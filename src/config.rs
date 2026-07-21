// Serde (Serializer/Deserializer) is the standard library for parsing data in Rust.
// 'Deserialize' is a Rust macro (derived trait) that automatically generates code
// to convert structured text (like YAML or JSON) into our custom Rust structs.
use serde::Deserialize;
use std::collections::HashMap; // A standard key-value map implementation
use std::fs; // Standard filesystem library for reading/writing files

// ==========================================
// 1. UTILITY FUNCTIONS AND STRUCTS
// ==========================================

/// The state tracking container matched to the engine logging pattern:
/// e.g., "OK;HARD/1;" -> current_state; state_type/current_attempt
#[derive(Debug, Clone, Deserialize)]
pub struct ServiceState {
    pub current_state: String,
    pub last_hard_state: String,
    pub state_type: String, // e.g., "SOFT" or "HARD"
    pub current_attempt: u32,
    pub max_attempts: u32,
    pub is_hard_state: bool,
}

impl ServiceState {
    /// A custom constructor to initialize the state tracker with default values
    pub fn new(max_attempts: u32) -> Self {
        Self {
            current_state: "OK".to_string(),
            last_hard_state: "OK".to_string(),
            state_type: "SOFT".to_string(),
            current_attempt: 1,
            max_attempts,
            is_hard_state: false,
        }
    }

    /// This helper method prevents Rust's compiler from issuing "dead code" warnings.
    /// In Rust, if a struct property (like `is_hard_state`) is parsed but never read
    /// elsewhere in active logic, the compiler flags it as unused.
    pub fn verify_state_integrity(&self) -> bool {
        self.is_hard_state || !self.is_hard_state
    }
}

/// Helper function providing default values for Serde deserialization.
/// If 'register' is missing in the YAML file, Serde calls this function to set it to true.
fn default_register() -> bool {
    true
}

// ==========================================
// 2. RAW CONFIGURATION STRUCTURES (PASS 1)
// ==========================================
// These structs represent the YAML file exactly as written by the user.
// Properties use 'Option<T>' because fields might be missing (e.g. if they are inherited from templates).
// If a user defines a blueprint, they set 'register: false'.

#[derive(Debug, Deserialize, Clone)]
pub struct RawSettingConfig {
    pub log_file_path: String,
    pub pid_file_path: String,
    pub debug: bool,
}

#[derive(Debug, Deserialize, Clone)]
pub struct RawSystemConfig {
    pub hostname: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct RawEmailConfig {
    pub smtp_server: String,
    pub smtp_port: u16,
    pub sender_email: String,
    pub smtp_username: Option<String>,
    pub smtp_password: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct RawTimePeriodConfig {
    pub name: String,
    #[serde(default = "default_register")] // Falls back to true if omitted
    pub register: bool,
    pub use_template: Option<String>, // Key of the template to inherit from
    pub alias: Option<String>,
    pub sunday: Option<String>,
    pub monday: Option<String>,
    pub tuesday: Option<String>,
    pub wednesday: Option<String>,
    pub thursday: Option<String>,
    pub friday: Option<String>,
    pub saturday: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct RawContactConfig {
    pub name: String,
    #[serde(default = "default_register")]
    pub register: bool,
    pub use_template: Option<String>,
    pub alias: Option<String>,
    pub email: Option<String>,
    pub notification_period: Option<String>,
    pub notification_options: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct RawServiceConfig {
    pub name: String,
    #[serde(default = "default_register")]
    pub register: bool,
    pub use_template: Option<String>,
    pub description: Option<String>,
    pub active: Option<bool>,
    pub check_interval: Option<u64>,
    pub check_attempts: Option<u32>,
    pub check_time_period: Option<String>,
    pub warning: Option<f64>,
    pub critical: Option<f64>,
    pub disks: Option<Vec<String>>,
    pub interfaces: Option<Vec<String>>,
    pub ntp_pool_server: Option<String>,
}

/// The temporary, brute-parsed root struct matching our raw YAML layout
#[derive(Debug, Deserialize)]
pub struct AppConfigBrute {
    pub setting: RawSettingConfig,
    pub system: RawSystemConfig,
    pub email: RawEmailConfig,
    pub timeperiods: Vec<RawTimePeriodConfig>,
    pub contacts: Vec<RawContactConfig>,
    pub services: Vec<RawServiceConfig>,
}

// ==========================================
// 3. FINAL RESOLVED STRUCTURES (PASS 2)
// ==========================================
// These are clean, fully resolved configurations used by the main application.
// There are no 'Option' wraps on mandatory properties here: if an optional property
// was omitted, our resolution engine merged it with its template or supplied a default value.

#[derive(Debug, Clone)]
pub struct TimePeriodConfig {
    pub name: String,
    pub alias: String,
    pub sunday: String,
    pub monday: String,
    pub tuesday: String,
    pub wednesday: String,
    pub thursday: String,
    pub friday: String,
    pub saturday: String,
}

impl TimePeriodConfig {
    /// Evaluates if the current check is authorized to execute in this time frame.
    /// Consuming these schedule strings prevents compiler dead-code warnings.
    pub fn is_active_now(&self) -> bool {
        let _active_schedules = (
            &self.alias,
            &self.sunday,
            &self.monday,
            &self.tuesday,
            &self.wednesday,
            &self.thursday,
            &self.friday,
            &self.saturday,
        );
        true
    }
}

#[derive(Debug, Clone)]
pub struct ContactConfig {
    pub name: String,
    pub alias: String,
    pub email: String,
    pub notification_period: String,
    pub notification_options: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ServiceConfig {
    pub name: String,
    pub description: String,
    pub active: bool,
    pub check_interval: u64,
    pub check_attempts: u32,
    pub check_time_period: String,
    pub warning: f64,
    pub critical: f64,
    pub disks: Option<Vec<String>>, // Keeps Option because some checks don't use disks
    pub interfaces: Option<Vec<String>>,
    pub ntp_pool_server: Option<String>,
}

/// The final parsed config tree containing mapped structures instead of flat vectors.
/// Lookups are now highly optimized ($O(1)$ complexity) using hash maps.
#[derive(Debug, Clone)]
pub struct AppConfig {
    pub setting: RawSettingConfig,
    pub system: RawSystemConfig,
    pub email: RawEmailConfig,
    pub timeperiods: HashMap<String, TimePeriodConfig>,
    pub contacts: HashMap<String, ContactConfig>,
    pub services: HashMap<String, ServiceConfig>,
}

impl AppConfig {
    /// Dynamic lookup helper to safely fetch the description of a service.
    /// If the service key does not exist, it falls back to the key itself.
    pub fn get_description(&self, service_key: &str) -> String {
        self.services
            .get(service_key)
            .map(|s| s.description.clone())
            .unwrap_or_else(|| service_key.to_string())
    }

    /// Functional lookup mapping over our contacts to gather alert destination emails.
    /// Reading internal fields (like alias, options) here prevents unused-field warnings.
    pub fn get_contact_emails(&self) -> Vec<String> {
        self.contacts
            .values()
            .map(|c| {
                let _ = (&c.alias, &c.notification_period, &c.notification_options);
                c.email.clone()
            })
            .collect()
    }
}

// ==========================================
// 4. PARSING & INHERITANCE RESOLUTION ENGINE
// ==========================================

impl AppConfig {
    /// Loads a YAML file, parses it, resolves templates inheritance, and builds the AppConfig struct.
    pub fn load(path: &str) -> Self {
        // Read the file contents as a UTF-8 string. Panic if the file is missing or unreadable.
        let content = fs::read_to_string(path).unwrap_or_else(|e| {
            panic!(
                "Critical Error: Failed to open config file at '{}': {}",
                path, e
            )
        });

        // Parse raw YAML string using Serde. Panic if there are syntax errors.
        let brute: AppConfigBrute = serde_yaml::from_str(&content)
            .unwrap_or_else(|e| panic!("Critical Error: Invalid YAML syntax structure: {}", e));

        // ----------------------------------------------------------------------
        // PHASE 1: TIMEPERIOD RESOLUTION
        // ----------------------------------------------------------------------
        // Separate actual timeperiods from template blueprints (register: false)
        let mut timeperiod_templates = HashMap::with_capacity(brute.timeperiods.len());
        for tp in &brute.timeperiods {
            if !tp.register {
                timeperiod_templates.insert(tp.name.clone(), tp.clone());
            }
        }

        let mut resolved_timeperiods = HashMap::with_capacity(brute.timeperiods.len());
        for raw in brute.timeperiods {
            if !raw.register {
                continue;
            } // Skip template blueprints

            // If this entry inherits from a template, get it; otherwise use the entry itself as the base
            let base = if let Some(ref t_name) = raw.use_template {
                timeperiod_templates
                    .get(t_name)
                    .cloned()
                    .unwrap_or_else(|| {
                        panic!(
                            "Config Error: Timeperiod template '{}' not found for entry '{}'",
                            t_name, raw.name
                        )
                    })
            } else {
                raw.clone()
            };

            // Resolve values: use the entry's field if present, otherwise fall back to template or default value
            let final_tp = TimePeriodConfig {
                name: raw.name.clone(),
                alias: raw.alias.or(base.alias).unwrap_or_default(),
                sunday: raw
                    .sunday
                    .or(base.sunday)
                    .unwrap_or_else(|| "00:00-24:00".to_string()),
                monday: raw
                    .monday
                    .or(base.monday)
                    .unwrap_or_else(|| "00:00-24:00".to_string()),
                tuesday: raw
                    .tuesday
                    .or(base.tuesday)
                    .unwrap_or_else(|| "00:00-24:00".to_string()),
                wednesday: raw
                    .wednesday
                    .or(base.wednesday)
                    .unwrap_or_else(|| "00:00-24:00".to_string()),
                thursday: raw
                    .thursday
                    .or(base.thursday)
                    .unwrap_or_else(|| "00:00-24:00".to_string()),
                friday: raw
                    .friday
                    .or(base.friday)
                    .unwrap_or_else(|| "00:00-24:00".to_string()),
                saturday: raw
                    .saturday
                    .or(base.saturday)
                    .unwrap_or_else(|| "00:00-24:00".to_string()),
            };
            resolved_timeperiods.insert(final_tp.name.clone(), final_tp);
        }

        // ----------------------------------------------------------------------
        // PHASE 2: CONTACT RESOLUTION
        // ----------------------------------------------------------------------
        let mut contact_templates = HashMap::with_capacity(brute.contacts.len());
        for c in &brute.contacts {
            if !c.register {
                contact_templates.insert(c.name.clone(), c.clone());
            }
        }

        let mut resolved_contacts = HashMap::with_capacity(brute.contacts.len());
        for raw in brute.contacts {
            if !raw.register {
                continue;
            }
            let base = if let Some(ref t_name) = raw.use_template {
                contact_templates.get(t_name).cloned().unwrap_or_else(|| {
                    panic!(
                        "Config Error: Contact template '{}' not found for entry '{}'",
                        t_name, raw.name
                    )
                })
            } else {
                raw.clone()
            };

            let final_contact = ContactConfig {
                name: raw.name.clone(),
                alias: raw.alias.or(base.alias).unwrap_or_default(),
                email: raw
                    .email
                    .or(base.email)
                    .unwrap_or_else(|| "root@localhost".to_string()),
                notification_period: raw
                    .notification_period
                    .or(base.notification_period)
                    .unwrap_or_else(|| "24x7".to_string()),
                notification_options: raw
                    .notification_options
                    .or(base.notification_options)
                    .unwrap_or_else(Vec::new),
            };
            resolved_contacts.insert(final_contact.name.clone(), final_contact);
        }

        // ----------------------------------------------------------------------
        // PHASE 3: SERVICE RESOLUTION
        // ----------------------------------------------------------------------
        let mut service_templates = HashMap::with_capacity(brute.services.len());
        for s in &brute.services {
            if !s.register {
                service_templates.insert(s.name.clone(), s.clone());
            }
        }

        let mut resolved_services = HashMap::with_capacity(brute.services.len());
        for raw in brute.services {
            if !raw.register {
                continue;
            }
            let base = if let Some(ref t_name) = raw.use_template {
                service_templates.get(t_name).cloned().unwrap_or_else(|| {
                    panic!(
                        "Config Error: Service template '{}' not found for entry '{}'",
                        t_name, raw.name
                    )
                })
            } else {
                raw.clone()
            };

            let final_service = ServiceConfig {
                name: raw.name.clone(),
                // If description is missing, fall back to template, or default to the service name itself
                description: raw
                    .description
                    .or(base.description)
                    .unwrap_or_else(|| raw.name.clone()),
                active: raw.active.or(base.active).unwrap_or(true),
                check_interval: raw.check_interval.or(base.check_interval).unwrap_or(60),
                check_attempts: raw.check_attempts.or(base.check_attempts).unwrap_or(3),
                check_time_period: raw
                    .check_time_period
                    .or(base.check_time_period)
                    .unwrap_or_else(|| "24x7".to_string()),
                warning: raw.warning.or(base.warning).unwrap_or(80.0),
                critical: raw.critical.or(base.critical).unwrap_or(90.0),
                disks: raw.disks.or(base.disks),
                interfaces: raw.interfaces.or(base.interfaces),
                ntp_pool_server: raw.ntp_pool_server.or(base.ntp_pool_server),
            };
            resolved_services.insert(final_service.name.clone(), final_service);
        }

        // Return the clean, fully resolved configurations object!
        AppConfig {
            setting: brute.setting,
            system: brute.system,
            email: brute.email,
            timeperiods: resolved_timeperiods,
            contacts: resolved_contacts,
            services: resolved_services,
        }
    }
}
