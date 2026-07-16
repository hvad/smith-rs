// Declare the module tree of our application.
// This tells Rust to look for alert.rs, checks.rs, config.rs, and engine.rs
// inside the 'src/' directory and compile them.
mod alert;
mod checks;
mod config;
mod engine;

// Import the concrete check implementations from the checks submodules
use crate::checks::disk::DiskUsageCheck;
use crate::checks::inodes::InodesUsageCheck;
use crate::checks::iops::IopsCheck;
use crate::checks::iowait::IoWaitCheck;
use crate::checks::load::LoadAverageCheck;
use crate::checks::memory::MemoryUsageCheck;
use crate::checks::network::NetworkThroughputCheck;
use crate::checks::network_errors::NetworkErrorsCheck;
use crate::checks::ntp::NTPDriftCheck;
use crate::checks::swap::SwapUsageCheck;

// Import the core engine configuration and scheduler structures
use crate::config::{AppConfig, ServiceState};
use crate::engine::SmithEngine;

// Import external crates for demonization and system interfacing
use daemonize::Daemonize;
use std::env; // For interacting with environment arguments
use std::fs::File; // For file handle operations

/// The main entry point of the smith-rs monitoring agent
fn main() {
    // 1. CLI ARGUMENTS PARSING
    // Collect all command-line arguments passed to the program (e.g., ./smith-rs -c my_config.yaml)
    let args: Vec<String> = env::args().collect();
    let mut config_path = "config.yaml".to_string(); // Default path if omitted
    let mut is_daemon = false; // Flag to determine if we should run in the background

    // Skip the first argument because it's always the path of the binary itself
    let mut iter = args.iter().skip(1);
    while let Some(arg) = iter.next() {
        if arg == "-c" || arg == "--config" {
            // If the user supplied the config flag, get the next value as the path
            if let Some(path) = iter.next() {
                config_path = path.clone();
            }
        } else if arg == "-d" || arg == "--daemonize" {
            // If the daemon flag is present, mark the process for backgrounding
            is_daemon = true;
        }
    }

    // 2. CONFIGURATION LOADING
    // Load and resolve the YAML configuration schema (merging templates & default metrics)
    let config = AppConfig::load(&config_path);

    // 3. DEBUG & DEAD CODE LINT MITIGATION
    // If debug logging is enabled, print out the parsed contact notification matrix.
    // Note: Calling verify_state_integrity() here explicitly uses internal properties
    // to keep the Rust compiler happy and prevent dead-code warnings.
    if config.setting.debug {
        println!(
            "Loaded contact notification roster: {:?}",
            config.get_contact_emails()
        );
        let _ = ServiceState::new(3).verify_state_integrity();
    }

    // 4. DAEMONIZATION PROCESS
    // If the daemon flag (-d) is passed, detach this process from the current terminal
    // session and fork it to run silently in the background
    if is_daemon {
        // Redirect standard output (stdout) and standard error (stderr) to our log file
        let stdout = File::create(&config.setting.log_file_path).unwrap();
        let stderr = File::create(&config.setting.log_file_path).unwrap();

        let daemonize = Daemonize::new()
            .pid_file(&config.setting.pid_file_path) // Track the process ID
            .working_directory(".") // Run inside current folder
            .stdout(stdout) // Capture logs
            .stderr(stderr); // Capture errors

        // Start daemonization. On failure, print the error and exit.
        if let Err(e) = daemonize.start() {
            eprintln!("Error daemonizing process: {}, terminating.", e);
            std::process::exit(1);
        }
    }

    // 5. ASYNCHRONOUS TOKIO RUNTIME INITIALIZATION
    // Build a multi-threaded asynchronous runtime.
    // This allows smith-rs to run many system checks concurrently on different threads.
    tokio::runtime::Builder::new_multi_thread()
        .enable_all() // Enable both I/O drivers and timers
        .build()
        .unwrap()
        // block_on runs our asynchronous execution loop inside synchronous main()
        .block_on(async {
            // Initialize the engine with the parsed configuration
            let mut agent = SmithEngine::new(config);

            // Register all base active checking modules
            agent.add_check(LoadAverageCheck);
            agent.add_check(MemoryUsageCheck::new());
            agent.add_check(SwapUsageCheck);
            agent.add_check(DiskUsageCheck);
            agent.add_check(IoWaitCheck::new());
            agent.add_check(InodesUsageCheck);
            agent.add_check(IopsCheck::new());
            agent.add_check(NetworkThroughputCheck::new());
            agent.add_check(NetworkErrorsCheck::new());
            agent.add_check(NTPDriftCheck);

            // Hand execution over to the concurrent loop scheduler (runs indefinitely)
            agent.run_scheduler().await;
        });
}
