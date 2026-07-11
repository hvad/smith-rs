pub mod disk;
pub mod inodes;
pub mod iops;
pub mod iowait;
pub mod load;
pub mod memory;
pub mod ntp;
pub mod swap;

use crate::config::AppConfig;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub enum CheckResult {
    Single { status: String, message: String },
    Multi(HashMap<String, (String, String)>),
}

#[async_trait::async_trait]
pub trait BaseCheck: Send + Sync {
    fn name(&self) -> &'static str;
    fn config_key(&self) -> &'static str;
    fn default_period(&self) -> u64;
    async fn run(&self, config: &AppConfig) -> Option<CheckResult>;
}
