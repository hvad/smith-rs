use ini::Ini;

#[derive(Clone)]
pub struct AppConfig {
    pub ini: Ini,
    pub hostname: String,
    pub log_file_path: String,
    pub pid_file_path: String,
}

impl AppConfig {
    pub fn load(path: &str) -> Self {
        let ini = Ini::load_from_file(path).expect("Failed to load configuration file");
        let hostname = ini
            .get_from(Some("System"), "hostname")
            .unwrap_or("localhost")
            .to_string();
        let log_file_path = ini
            .get_from(Some("Setting"), "log_file_path")
            .unwrap_or("smith-rs.log")
            .to_string();
        let pid_file_path = ini
            .get_from(Some("Setting"), "pid_file_path")
            .unwrap_or("smith-rs.pid")
            .to_string();

        AppConfig {
            ini,
            hostname,
            log_file_path,
            pid_file_path,
        }
    }
}
