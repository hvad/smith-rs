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

    // Extracted value tokens converted into clean Nagios performance metrics payload parameters
    fn extract_perf_data(&self, check_key: &str, message: &str) -> String {
        let (warn, crit) = self
            .config
            .services
            .get(check_key)
            .map(|sc| (sc.warning, sc.critical))
            .unwrap_or((0.0, 0.0));

        match check_key {
            "load" => {
                let mut numbers = message.split(',').map(|s| {
                    s.split(':')
                        .nth(1)
                        .unwrap_or("0")
                        .trim()
                        .parse::<f64>()
                        .unwrap_or(0.0)
                });
                if let (Some(n1), Some(n2), Some(n3)) =
                    (numbers.next(), numbers.next(), numbers.next())
                {
                    format!("load1={:.2};{:.2};{:.2};0; load5={:.2};{:.2};{:.2};0; load15={:.2};{:.2};{:.2};0", 
                        n1, warn, crit, n2, warn, crit, n3, warn, crit)
                } else {
                    String::new()
                }
            }
            "memory" | "swap" | "iowait" => {
                if let Some(pct_str) = message.split(':').nth(1) {
                    let pct = pct_str
                        .replace('%', "")
                        .trim()
                        .parse::<f64>()
                        .unwrap_or(0.0);
                    format!("{}={:.2}%;{:.2};{:.2};0;100", check_key, pct, warn, crit)
                } else {
                    String::new()
                }
            }
            "disk" | "inodes" => {
                let pct = message
                    .split_whitespace()
                    .find(|s| s.contains('%'))
                    .map(|s| s.replace('%', "").parse::<f64>().unwrap_or(0.0))
                    .unwrap_or(0.0);
                format!("utilization={:.2}%;{:.2};{:.2};0;100", pct, warn, crit)
            }
            "iops" => {
                let total = message
                    .split_whitespace()
                    .nth(2)
                    .unwrap_or("0")
                    .parse::<f64>()
                    .unwrap_or(0.0);
                format!("iops={:.1};{:.1};{:.1};0", total, warn, crit)
            }
            "network" => {
                let total = message
                    .split_whitespace()
                    .nth(1)
                    .unwrap_or("0")
                    .parse::<f64>()
                    .unwrap_or(0.0);
                format!("throughput={:.2}Mbps;{:.2};{:.2};0", total, warn, crit)
            }
            "network_errors" => {
                let mut parts = message.split(',');
                let err_rate = parts
                    .next()
                    .unwrap_or("")
                    .split('(')
                    .nth(1)
                    .unwrap_or("0")
                    .replace("/s)", "")
                    .parse::<f64>()
                    .unwrap_or(0.0);
                let drop_rate = parts
                    .next()
                    .unwrap_or("")
                    .split('(')
                    .nth(1)
                    .unwrap_or("0")
                    .replace("/s)", "")
                    .parse::<f64>()
                    .unwrap_or(0.0);
                format!(
                    "errors_rate={:.2}/s;{:.2};{:.2};0; dropped_rate={:.2}/s;{:.2};{:.2};0",
                    err_rate, warn, crit, drop_rate, warn, crit
                )
            }
            "ntp" => {
                let offset = message
                    .split_whitespace()
                    .nth(1)
                    .unwrap_or("0")
                    .replace('s', "")
                    .parse::<f64>()
                    .unwrap_or(0.0);
                format!("offset={:.4}s;{:.4};{:.4};0", offset, warn, crit)
            }
            _ => String::new(),
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

        // Dynamic lookup to ensure multi-underscore keys pull the correct maximum verification attempts limit
        let root_key = if check_key.starts_with("network_errors") {
            "network_errors"
        } else if check_key.starts_with("tcp_states") {
            "tcp_states"
        } else {
            check_key.split('_').next().unwrap_or(check_key)
        };

        let max_attempts = self
            .config
            .services
            .get(root_key)
            .map(|sc| sc.check_attempts)
            .unwrap_or(3);
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

        let service_desc = self.config.get_description(root_key);
        let display_name = if category.contains('(') {
            category
                .split(" (")
                .nth(1)
                .map(|sub| format!("{} ({}", service_desc, sub))
                .unwrap_or(service_desc)
        } else {
            service_desc
        };

        let perf_data = self.extract_perf_data(root_key, message);
        let msg_with_perf = if perf_data.is_empty() {
            message.to_string()
        } else {
            format!("{} | {}", message, perf_data)
        };

        let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        let log_entry = format!(
            "{} - SERVICE : {};{};{};{}/{};{}\n",
            timestamp,
            self.config.system.hostname,
            display_name,
            state.current_state,
            state.state_type,
            state.current_attempt,
            msg_with_perf
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
            if self.config.setting.debug {
                let debug_log = format!("{} - [DEBUG] : {};{};{};Alert trigger bypassed (SMTP suppressed in debug mode)\n",
                    timestamp, self.config.system.hostname, display_name, state.last_hard_state);
                print!("{}", debug_log);
                if let Ok(mut file) = OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&self.config.setting.log_file_path)
                {
                    let _ = file.write_all(debug_log.as_bytes());
                }
            } else {
                let notification_log = format!(
                    "{} - NOTIFICATION : {};{};{};{}\n",
                    timestamp,
                    self.config.system.hostname,
                    display_name,
                    state.last_hard_state,
                    msg_with_perf
                );
                print!("{}", notification_log);
                if let Ok(mut file) = OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&self.config.setting.log_file_path)
                {
                    let _ = file.write_all(notification_log.as_bytes());
                }

                let email_alert_clone = Arc::clone(&self.email_alert);
                let ck = check_key.to_string();
                let cat = display_name;
                let st = state.last_hard_state.clone();
                let msg = msg_with_perf.clone();
                let log_file_path = self.config.setting.log_file_path.clone();
                let hostname = self.config.system.hostname.clone();

                tokio::spawn(async move {
                    if let Err(err) = email_alert_clone
                        .send_nagios_hard_alert(&ck, &cat, &st, &msg)
                        .await
                    {
                        let error_log = format!(
                            "{} - ERROR : {};{};Notification failed to send: {}\n",
                            Local::now().format("%Y-%m-%d %H:%M:%S"),
                            hostname,
                            cat,
                            err
                        );
                        print!("{}", error_log);
                        if let Ok(mut file) = OpenOptions::new()
                            .create(true)
                            .append(true)
                            .open(&log_file_path)
                        {
                            let _ = file.write_all(error_log.as_bytes());
                        }
                    }
                });
            }
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
                    self.handle_state_transition(
                        &format!("{}_{}", check_key, item),
                        &format!("{} ({})", category, item),
                        &status,
                        &msg,
                    )
                    .await;
                }
            }
        }
    }

    pub async fn run_scheduler(self) {
        let engine = Arc::new(self);
        let mut worker_handles = Vec::with_capacity(engine.checks.len());

        for index in 0..engine.checks.len() {
            let engine_clone = Arc::clone(&engine);
            let check_key = engine_clone.checks[index].config_key();
            let check_name = engine_clone.checks[index].name();

            let (period_secs, check_time_period) = engine_clone
                .config
                .services
                .get(check_key)
                .map(|sc| (sc.check_interval, sc.check_time_period.clone()))
                .unwrap_or_else(|| {
                    (
                        engine_clone.checks[index].default_period(),
                        "24x7".to_string(),
                    )
                });

            let handle = tokio::spawn(async move {
                loop {
                    let loop_start = tokio::time::Instant::now();
                    let can_run = engine_clone
                        .config
                        .timeperiods
                        .get(&check_time_period)
                        .map(|tp| tp.is_active_now())
                        .unwrap_or(true);

                    if can_run {
                        if let Some(check_result) =
                            engine_clone.checks[index].run(&engine_clone.config).await
                        {
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
