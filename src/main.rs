mod alert;
mod checks;
mod config;
mod engine;

use crate::checks::disk::DiskUsageCheck;
use crate::checks::inodes::InodesUsageCheck;
use crate::checks::iops::IopsCheck;
use crate::checks::iowait::IoWaitCheck;
use crate::checks::load::LoadAverageCheck;
use crate::checks::memory::MemoryUsageCheck;
use crate::checks::ntp::NTPDriftCheck;
use crate::checks::swap::SwapUsageCheck;
use crate::config::AppConfig;
use crate::engine::SmithEngine;

use daemonize::Daemonize;
use std::env;
use std::fs::File;

fn main() {
    let args: Vec<String> = env::args().collect();
    let mut config_path = "config.yaml".to_string();
    let mut is_daemon = false;

    for i in 0..args.len() {
        if (args[i] == "-c" || args[i] == "--config") && i + 1 < args.len() {
            config_path = args[i + 1].clone();
        }
        if args[i] == "-d" || args[i] == "--daemonize" {
            is_daemon = true;
        }
    }

    let config = AppConfig::load(&config_path);

    if is_daemon {
        let stdout = File::create(&config.setting.log_file_path).unwrap();
        let stderr = File::create(&config.setting.log_file_path).unwrap();

        let daemonize = Daemonize::new()
            .pid_file(&config.setting.pid_file_path)
            .working_directory(".")
            .stdout(stdout)
            .stderr(stderr);

        match daemonize.start() {
            Ok(_) => {}
            Err(e) => {
                eprintln!("Error daemonizing process: {}, terminating.", e);
                std::process::exit(1);
            }
        }
    }

    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(async {
        let mut agent = SmithEngine::new(config);

        agent.add_check(LoadAverageCheck);
        agent.add_check(MemoryUsageCheck::new());
        agent.add_check(SwapUsageCheck);
        agent.add_check(DiskUsageCheck);
        agent.add_check(NTPDriftCheck);
        agent.add_check(IoWaitCheck::new());
        agent.add_check(InodesUsageCheck);
        agent.add_check(IopsCheck::new());

        agent.run_scheduler().await;
    });
}
