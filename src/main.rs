// Declare the module hierarchy.
// Rust will resolve these modules by looking for corresponding .rs files inside src/
mod alert;
mod checks;
mod config;
mod engine;

use std::fs::OpenOptions;
use std::process;

// Import clap for clean CLI argument handling
use clap::Parser;
// Import daemonize crate for running the process in the background on Unix systems
use daemonize::Daemonize;

// Import concrete metric checks
use crate::checks::{
    disk::DiskUsageCheck, inodes::InodesUsageCheck, iops::IopsCheck, iowait::IoWaitCheck,
    load::LoadAverageCheck, memory::MemoryUsageCheck, network::NetworkCheck,
    network_errors::NetworkErrorsCheck, ntp::NTPDriftCheck, swap::SwapUsageCheck,
};
use crate::config::AppConfig;
use crate::engine::SmithEngine;

/// Command-line arguments structure parsed automatically by Clap.
/// The `#[derive(Parser)]` macro generates all CLI parsing logic at compile time.
#[derive(Parser, Debug)]
#[command(
    author,
    version,
    about = "smith-rs - A lightweight, asynchronous system monitoring agent",
    long_about = None
)]
struct Args {
    /// Path to the YAML configuration file
    #[arg(short, long, default_value = "config.yaml")]
    config: String,

    /// Detach process and run as a background daemon
    #[arg(short, long, default_value_t = false)]
    daemonize: bool,
}

fn main() {
    // ------------------------------------------------------------------------
    // 1. CLI ARGUMENTS PARSING
    // ------------------------------------------------------------------------
    // Parse arguments using clap. This handles --help, --version, and flags safely.
    let args = Args::parse();

    // ------------------------------------------------------------------------
    // 2. CONFIGURATION LOADING
    // ------------------------------------------------------------------------
    // Read and parse the YAML config file into our strongly-typed Rust struct.
    let config = AppConfig::load(&args.config);

    if config.setting.debug {
        println!(
            "Loaded contact notification roster: {:?}",
            config.get_contact_emails()
        );
    }

    // ------------------------------------------------------------------------
    // 3. DAEMONIZATION PROCESS
    // ------------------------------------------------------------------------
    // If the -d or --daemonize flag was supplied, detach from the terminal.
    // CRITICAL: Daemonization MUST happen BEFORE starting Tokio multi-threading
    // to avoid undefined behavior caused by Unix fork() on multi-threaded processes.
    if args.daemonize {
        // Open the log file in APPEND mode so previous logs are not overwritten
        let log_file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&config.setting.log_file_path)
            .unwrap_or_else(|err| {
                eprintln!(
                    "Fatal: Failed to open log file at '{}': {}",
                    config.setting.log_file_path, err
                );
                process::exit(1);
            });

        // Clone the file descriptor for stderr to avoid opening duplicate handles
        let stderr_file = log_file.try_clone().unwrap_or_else(|err| {
            eprintln!("Fatal: Failed to clone log file handle for stderr: {}", err);
            process::exit(1);
        });

        let daemonize = Daemonize::new()
            .pid_file(&config.setting.pid_file_path)
            .working_directory(".")
            .stdout(log_file)
            .stderr(stderr_file);

        if let Err(err) = daemonize.start() {
            eprintln!("Fatal: Failed to daemonize process: {}", err);
            process::exit(1);
        }
    }

    // ------------------------------------------------------------------------
    // 4. TOKIO ASYNCHRONOUS RUNTIME INITIALIZATION
    // ------------------------------------------------------------------------
    // Create a multi-threaded async runtime to schedule monitoring tasks concurrently.
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all() // Enable both I/O drivers and timers
        .build()
        .unwrap_or_else(|err| {
            eprintln!("Fatal: Failed to build Tokio async runtime: {}", err);
            process::exit(1);
        });

    // Run the main async task inside the synchronous main function
    runtime.block_on(async {
        // Instantiate the engine with our application configuration
        let mut agent = SmithEngine::new(config);

        // Register all active metric check modules
        agent.add_check(LoadAverageCheck);
        agent.add_check(MemoryUsageCheck::new());
        agent.add_check(SwapUsageCheck);
        agent.add_check(DiskUsageCheck);
        agent.add_check(IoWaitCheck::new());
        agent.add_check(InodesUsageCheck);
        agent.add_check(IopsCheck::new());
        agent.add_check(NetworkCheck::new());
        agent.add_check(NetworkErrorsCheck::new());
        agent.add_check(NTPDriftCheck);

        // Start the infinite scheduling loop
        agent.run_scheduler().await;
    });
}
