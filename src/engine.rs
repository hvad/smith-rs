// Import necessary structures and modules from our alert, checks, and config files
use crate::alert::SMTPAlert;
use crate::checks::{BaseCheck, CheckResult};
use crate::config::{AppConfig, ServiceState};

// Import standard library and external utility crates
use chrono::Local; // For generating timestamps in human-readable formats
use std::collections::HashMap; // A standard key-value map implementation
use std::fs::OpenOptions; // Used to safely open files with append/write flags
use std::io::Write; // Trait required to write raw byte slices to files
use std::sync::Arc; // "Atomically Reference Counted" pointer for shared thread-safe ownership
use std::time::Duration; // Represents spans of time (e.g., seconds)
use tokio::sync::Mutex; // Asynchronous Mutex to safely coordinate writes to states across threads

/// The core engine struct responsible for orchestrating background monitoring tasks
pub struct SmithEngine {
    config: AppConfig,           // Application configurations read from the YAML file
    email_alert: Arc<SMTPAlert>, // Thread-safe shared pointer to our alert dispatching module
    // A thread-safe vector containing checked instances.
    // `Box<dyn BaseCheck>` means "a heap-allocated object implementing the BaseCheck trait" (polymorphism)
    checks: Arc<Vec<Box<dyn BaseCheck>>>,
    // A thread-safe, asynchronously locked map tracking the execution state of each service check
    states: Arc<Mutex<HashMap<String, ServiceState>>>,
}

impl SmithEngine {
    /// Constructor to initialize a fresh engine instance
    pub fn new(config: AppConfig) -> Self {
        // Create the alert helper. It takes a reference, but we wrap the engine's reference in an Arc
        let email_alert = Arc::new(SMTPAlert::new(&config));
        SmithEngine {
            config,
            email_alert,
            checks: Arc::new(Vec::new()),
            states: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Adds a monitoring check to the scheduler loop if the check is active in the configuration
    pub fn add_check<T: BaseCheck + 'static>(&mut self, check: T) {
        // Retrieve the service configuration using the check's unique identifier key
        if let Some(service_conf) = self.config.services.get(check.config_key()) {
            // Only load the check if 'active' is set to true in the configuration
            if service_conf.active {
                // Arc::get_mut checks if there's only 1 reference to `checks`.
                // This is safe during initialization because scheduler threads haven't spawned yet.
                Arc::get_mut(&mut self.checks)
                    .expect("SmithEngine initialization requires exclusive access")
                    .push(Box::new(check));
            }
        }
    }

    /// Generates standard Nagios performance data (perfdata) from check messages.
    /// Format: 'label'=value[UOM];[warn];[crit];[min];[max]
    fn extract_perf_data(&self, check_key: &str, message: &str) -> String {
        // Fetch threshold limits configured for this specific service profile
        let (warn, crit) = self
            .config
            .services
            .get(check_key)
            .map(|sc| (sc.warning, sc.critical))
            .unwrap_or((0.0, 0.0));

        // Use pattern matching on the service key to parse strings and format the perfdata payload
        match check_key {
            "load" => {
                // Input format example: "1min: 1.81, 5min: 1.77, 15min: 2.22"
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
                // Input format example: "RAM Usage: 45.12%"
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
                // Input format example: "Used: 62.45%"
                let pct = message
                    .split_whitespace()
                    .find(|s| s.contains('%'))
                    .map(|s| s.replace('%', "").parse::<f64>().unwrap_or(0.0))
                    .unwrap_or(0.0);
                format!("utilization={:.2}%;{:.2};{:.2};0;100", pct, warn, crit)
            }
            "iops" => {
                // Input format example: "Total IOPS: 150.2 (Read: 50.0/s, Write: 100.2/s)"
                let total = message
                    .split_whitespace()
                    .nth(2)
                    .unwrap_or("0")
                    .parse::<f64>()
                    .unwrap_or(0.0);
                format!("iops={:.1};{:.1};{:.1};0", total, warn, crit)
            }
            "network" => {
                // Input format example: "Throughput: 45.20 Mbps (RX: 20.10 Mbps, TX: 25.10 Mbps)"
                let total = message
                    .split_whitespace()
                    .nth(1)
                    .unwrap_or("0")
                    .parse::<f64>()
                    .unwrap_or(0.0);
                format!("throughput={:.2}Mbps;{:.2};{:.2};0", total, warn, crit)
            }
            "network_errors" => {
                // Input format example: "Errors: 2 (0.10/s), Dropped: 0 (0.00/s)"
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
                // Input format example: "Offset: 0.0012s"
                let offset = message
                    .split_whitespace()
                    .nth(1)
                    .unwrap_or("0")
                    .replace('s', "")
                    .parse::<f64>()
                    .unwrap_or(0.0);
                format!("offset={:.4}s;{:.4};{:.4};0", offset, warn, crit)
            }
            _ => String::new(), // Return an empty string if no performance parsing parser is configured
        }
    }

    /// Processes transitions of state checks, writes to the log file, and initiates notifications
    async fn handle_state_transition(
        &self,
        check_key: &str,
        category: &str,
        status: &str,
        message: &str,
    ) {
        // Lock the shared states map to make sure no other thread modifies it at the same time
        let mut states_guard = self.states.lock().await;

        // For sub-element checks (e.g. network_errors_eth0), find the root key to get correct config
        let root_key = if check_key.starts_with("network_errors") {
            "network_errors"
        } else if check_key.starts_with("tcp_states") {
            "tcp_states"
        } else {
            check_key.split('_').next().unwrap_or(check_key)
        };

        // Get max check attempts from config, defaulting to 3
        let max_attempts = self
            .config
            .services
            .get(root_key)
            .map(|sc| sc.check_attempts)
            .unwrap_or(3);

        // Get the existing state tracker or initialize a new default one for this check
        let state = states_guard
            .entry(check_key.to_string())
            .or_insert_with(|| ServiceState::new(max_attempts));
        let mut notify = false;

        // NAGIOS-LIKE STATE MACHINE:
        // Decides if a state is SOFT (temporary/retrying) or HARD (confirmed error)
        if status == state.current_state {
            if state.state_type == "SOFT" {
                state.current_attempt += 1;
                if state.current_attempt >= state.max_attempts {
                    state.state_type = "HARD".to_string();
                    if state.current_state != state.last_hard_state {
                        state.last_hard_state = state.current_state.clone();
                        notify = true; // Trigger alert notification!
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
                } // Trigger alert recovery!
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

        // Retrieve description text from config
        let service_desc = self.config.get_description(root_key);
        // Format sub-items nicely, e.g., "Disk Space (/)" instead of "disk_/"
        let display_name = if category.contains('(') {
            category
                .split(" (")
                .nth(1)
                .map(|sub| format!("{} ({}", service_desc, sub))
                .unwrap_or(service_desc)
        } else {
            service_desc
        };

        // Extract and format performance data (perfdata)
        let perf_data = self.extract_perf_data(root_key, message);
        let msg_with_perf = if perf_data.is_empty() {
            message.to_string()
        } else {
            format!("{} | {}", message, perf_data)
        };

        // Print standard execution log entry
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

        // PROCESS NOTIFICATIONS: Only runs if 'notify' flag was flipped to true
        if notify {
            if self.config.setting.debug {
                // If debug mode is active, do not send email. Write [DEBUG] and bypass SMTP.
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
                // Normal Production Flow: Write notification log line and attempt email dispatch
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

                // Prepare variables for the asynchronous thread
                let email_alert_clone = Arc::clone(&self.email_alert); // Increase reference count of AlertSystem
                let ck = check_key.to_string();
                let cat = display_name;
                let st = state.last_hard_state.clone();
                let msg = msg_with_perf.clone();
                let log_file_path = self.config.setting.log_file_path.clone();
                let hostname = self.config.system.hostname.clone();

                // Spawn a lightweight background thread (tokio green-thread) to send the email alert
                // without blocking the main monitoring engine execution loop
                tokio::spawn(async move {
                    if let Err(err) = email_alert_clone
                        .send_nagios_hard_alert(&ck, &cat, &st, &msg)
                        .await
                    {
                        // If SMTP fails, catch the error and write an [ERROR] line to the log file
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

    /// Evaluates execution payloads returned by our check modules
    async fn process_result(&self, check_key: &str, category: &str, result: CheckResult) {
        match result {
            // For a single metric, proceed directly to state transition
            CheckResult::Single { status, message } => {
                self.handle_state_transition(check_key, category, &status, &message)
                    .await;
            }
            // For a multi-metric check (like checking multiple hard disks)
            CheckResult::Multi(map) => {
                for (item, (status, msg)) in map {
                    // Unique check subkeys are created (e.g. disk_sda) to track each element independently
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

    /// Runs the scheduling loop to execute active checks concurrently at designated intervals
    pub async fn run_scheduler(self) {
        // Wrap 'self' in an Arc so scheduler threads can share the engine reference safely
        let engine = Arc::new(self);
        let mut worker_handles = Vec::with_capacity(engine.checks.len());

        // Spawn a monitoring loop for each registered check
        for index in 0..engine.checks.len() {
            let engine_clone = Arc::clone(&engine);
            let check_key = engine_clone.checks[index].config_key();
            let check_name = engine_clone.checks[index].name();

            // Resolve check execution intervals from configuration settings
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

            // Spawn an independent task loop for this specific check
            let handle = tokio::spawn(async move {
                loop {
                    let loop_start = tokio::time::Instant::now();

                    // Verify if the check is allowed to run within the current time window
                    let can_run = engine_clone
                        .config
                        .timeperiods
                        .get(&check_time_period)
                        .map(|tp| tp.is_active_now())
                        .unwrap_or(true);

                    if can_run {
                        // Execute the asynchronous check
                        if let Some(check_result) =
                            engine_clone.checks[index].run(&engine_clone.config).await
                        {
                            engine_clone
                                .process_result(check_key, check_name, check_result)
                                .await;
                        }
                    }

                    // Calculate elapsed execution time to maintain precise checking intervals
                    let elapsed = loop_start.elapsed();
                    let interval = Duration::from_secs(period_secs);
                    if elapsed < interval {
                        tokio::time::sleep(interval - elapsed).await;
                    }
                }
            });
            // Keep track of task handles so we can keep the program running
            worker_handles.push(handle);
        }

        // Await all spawned scheduler handles (runs infinitely unless interrupted)
        for handle in worker_handles {
            let _ = handle.await;
        }
    }
}
