use chrono::{Datelike, Local, Timelike};
use serde::Deserialize;
use std::collections::HashMap;
use std::fs::File;

#[derive(Debug, Deserialize, Clone)]
pub struct SettingConfig {
    pub log_file_path: String,
    pub pid_file_path: String,
    #[allow(dead_code)]
    pub debug: Option<bool>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct SystemConfig {
    pub hostname: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct EmailConfig {
    pub smtp_server: String,
    pub smtp_port: u16,
    pub sender_email: String,
    pub smtp_username: Option<String>,
    pub smtp_password: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct TimePeriod {
    pub name: String,
    #[allow(dead_code)]
    pub alias: String,
    pub sunday: Option<String>,
    pub monday: Option<String>,
    pub tuesday: Option<String>,
    pub wednesday: Option<String>,
    pub thursday: Option<String>,
    pub friday: Option<String>,
    pub saturday: Option<String>,
}

impl TimePeriod {
    pub fn is_active_now(&self) -> bool {
        let now = Local::now();
        let weekday = now.weekday().num_days_from_sunday();
        let current_minutes = now.hour() * 60 + now.minute();

        let day_str = match weekday {
            0 => &self.sunday,
            1 => &self.monday,
            2 => &self.tuesday,
            3 => &self.wednesday,
            4 => &self.thursday,
            5 => &self.friday,
            6 => &self.saturday,
            _ => &None,
        };

        if let Some(range) = day_str {
            for token in range.split(',') {
                let parts: Vec<&str> = token.trim().split('-').collect();
                if parts.len() == 2 {
                    let start_parts: Vec<&str> = parts[0].split(':').collect();
                    let end_parts: Vec<&str> = parts[1].split(':').collect();
                    if start_parts.len() == 2 && end_parts.len() == 2 {
                        let start_h = start_parts[0].parse::<u32>().unwrap_or(0);
                        let start_m = start_parts[1].parse::<u32>().unwrap_or(0);
                        let end_h = end_parts[0].parse::<u32>().unwrap_or(24);
                        let end_m = end_parts[1].parse::<u32>().unwrap_or(0);

                        let start_total = start_h * 60 + start_m;
                        let end_total = if parts[1].trim() == "24:00" {
                            24 * 60
                        } else {
                            end_h * 60 + end_m
                        };

                        if current_minutes >= start_total && current_minutes <= end_total {
                            return true;
                        }
                    }
                }
            }
        }
        false
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct Contact {
    pub name: String,
    pub alias: String,
    pub email: String,
    pub notification_period: String,
    pub notification_options: Vec<String>,
}

impl Contact {
    pub fn wants_notification(&self, status: &str, periods: &HashMap<String, TimePeriod>) -> bool {
        if let Some(tp) = periods.get(&self.notification_period) {
            if !tp.is_active_now() {
                return false;
            }
        } else {
            return false;
        }

        let option_flag = match status {
            "WARNING" => "w",
            "CRITICAL" => "c",
            "OK" => "r",
            _ => "u",
        };
        self.notification_options
            .iter()
            .any(|opt| opt.trim() == option_flag)
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct ServiceConfig {
    pub name: String,
    pub description: String,
    pub active: bool,
    pub check_interval: u64,
    pub check_attempts: u32,
    pub check_time_period: String,
    pub warning: f64,
    pub critical: f64,
    pub disks: Option<Vec<String>>,
    pub ntp_pool_server: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ServiceState {
    pub current_state: String,
    pub last_hard_state: String,
    pub state_type: String,
    pub current_attempt: u32,
    pub max_attempts: u32,
}

impl ServiceState {
    pub fn new(max_attempts: u32) -> Self {
        ServiceState {
            current_state: "OK".to_string(),
            last_hard_state: "OK".to_string(),
            state_type: "HARD".to_string(),
            current_attempt: 1,
            max_attempts,
        }
    }
}

#[derive(Debug, Deserialize)]
struct YamlConfig {
    setting: SettingConfig,
    system: SystemConfig,
    email: EmailConfig,
    timeperiods: Vec<TimePeriod>,
    contacts: Vec<Contact>,
    services: Vec<ServiceConfig>,
}

#[derive(Clone)]
pub struct AppConfig {
    pub setting: SettingConfig,
    pub system: SystemConfig,
    pub email: EmailConfig,
    pub timeperiods: HashMap<String, TimePeriod>,
    pub contacts: HashMap<String, Contact>,
    pub services: HashMap<String, ServiceConfig>,
}

impl AppConfig {
    pub fn load(path: &str) -> Self {
        let file = File::open(path).expect("Failed to open configuration file");
        let raw_config: YamlConfig =
            serde_yaml::from_reader(file).expect("Failed to parse YAML configuration");

        let mut timeperiods = HashMap::new();
        for tp in raw_config.timeperiods {
            timeperiods.insert(tp.name.clone(), tp);
        }

        let mut contacts = HashMap::new();
        for c in raw_config.contacts {
            contacts.insert(c.name.clone(), c);
        }

        let mut services = HashMap::new();
        for s in raw_config.services {
            services.insert(s.name.clone(), s);
        }

        AppConfig {
            setting: raw_config.setting,
            system: raw_config.system,
            email: raw_config.email,
            timeperiods,
            contacts,
            services,
        }
    }
}
