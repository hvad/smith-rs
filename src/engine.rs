use crate::alert::SMTPAlert;
use crate::checks::{BaseCheck, CheckResult};
use crate::config::{AppConfig, ServiceState};

use chrono::Local;
use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::Write;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

pub struct SmithEngine {
    config: AppConfig,
    email_alert: Arc<SMTPAlert>,
    checks: Arc<Vec<Box<dyn BaseCheck>>>,
    states: Arc<Mutex<HashMap<String, ServiceState>>>,
}

impl SmithEngine {
    pub fn new(config: AppConfig) -> Self {
        let email_alert = Arc::new(SMTPAlert::new(&config));
        SmithEngine {
            config,
            email_alert,
            checks: Arc::new(Vec::new()),
            states: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn add_check<T: BaseCheck + 'static>(&mut self, check: T) {
        if let Some(service_conf) = self.config.services.get(check.config_key()) {
            if service_conf.active {
                Arc::get_mut(&mut self.checks)
                    .expect("SmithEngine initialization requires exclusive access")
                    .push(Box::new(check));
            }
        }
    }

    async fn handle_state_transition(
        &self,
        check_key: &str,
        category: &str,
        status: &str,
        message: &str,
    ) {
        let mut states_guard = self.states.lock().await;

        let max_attempts = if let Some(sc) = self.config.services.get(check_key) {
            sc.check_attempts
        } else {
            3
        };

        let state = states_guard
            .entry(check_key.to_string())
            .or_insert_with(|| ServiceState::new(max_attempts));
        let mut notify = false;

        if status == state.current_state {
            if state.state_type == "SOFT" {
                state.current_attempt += 1;
                if state.current_attempt >= state.max_attempts {
                    state.state_type = "HARD".to_string();
                    if state.current_state != state.last_hard_state {
                        state.last_hard_state = state.current_state.clone();
                        notify = true;
                    }
                }
            }
        } else {
            state.current_state = status.to_string();

            if status == "OK" {
                let was_hard_error = state.last_hard_state != "OK";
                state.state_type = "HARD".to_string();
                state.current_attempt = 1;
                state.last_hard_state = "OK".to_string();
                if was_hard_error {
                    notify = true;
                }
            } else {
                state.state_type = "SOFT".to_string();
                state.current_attempt = 1;

                if state.max_attempts <= 1 {
                    state.state_type = "HARD".to_string();
                    state.last_hard_state = state.current_state.clone();
                    notify = true;
                }
            }
        }

        let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        let log_entry = format!(
            "{} - SERVICE : {};{};{};{}/{};{}\n",
            timestamp,
            self.config.system.hostname,
            category,
            state.current_state,
            state.state_type,
            state.current_attempt,
            message
        );

        print!("{}", log_entry);
        if let Ok(mut file) = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.config.setting.log_file_path)
        {
            let _ = file.write_all(log_entry.as_bytes());
        }

        if notify {
            let email_alert_clone = Arc::clone(&self.email_alert);
            let ck = check_key.to_string();
            let cat = category.to_string();
            let st = state.last_hard_state.clone();
            let msg = message.to_string();
            tokio::spawn(async move {
                email_alert_clone
                    .send_nagios_hard_alert(&ck, &cat, &st, &msg)
                    .await;
            });
        }
    }

    async fn process_result(&self, check_key: &str, category: &str, result: CheckResult) {
        match result {
            CheckResult::Single { status, message } => {
                self.handle_state_transition(check_key, category, &status, &message)
                    .await;
            }
            CheckResult::Multi(map) => {
                for (item, (status, msg)) in map {
                    let sub_key = format!("{}_{}", check_key, item);
                    let sub_category = format!("{} ({})", category, item);
                    self.handle_state_transition(&sub_key, &sub_category, &status, &msg)
                        .await;
                }
            }
        }
    }

    pub async fn run_scheduler(self) {
        let engine = Arc::new(self);
        let mut worker_handles = Vec::new();
        let total_checks = engine.checks.len();

        for index in 0..total_checks {
            let engine_clone = Arc::clone(&engine);
            let check_key = engine_clone.checks[index].config_key();
            let check_name = engine_clone.checks[index].name();

            let (period_secs, check_time_period) =
                if let Some(sc) = engine_clone.config.services.get(check_key) {
                    (sc.check_interval, sc.check_time_period.clone())
                } else {
                    (
                        engine_clone.checks[index].default_period(),
                        "24x7".to_string(),
                    )
                };

            let handle = tokio::spawn(async move {
                loop {
                    let loop_start = tokio::time::Instant::now();

                    let can_run =
                        if let Some(tp) = engine_clone.config.timeperiods.get(&check_time_period) {
                            tp.is_active_now()
                        } else {
                            true
                        };

                    if can_run {
                        let res = engine_clone.checks[index].run(&engine_clone.config).await;
                        if let Some(check_result) = res {
                            engine_clone
                                .process_result(check_key, check_name, check_result)
                                .await;
                        }
                    }

                    let elapsed = loop_start.elapsed();
                    let interval = Duration::from_secs(period_secs);
                    if elapsed < interval {
                        tokio::time::sleep(interval - elapsed).await;
                    }
                }
            });
            worker_handles.push(handle);
        }

        for handle in worker_handles {
            let _ = handle.await;
        }
    }
}
