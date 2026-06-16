# smith-rs

`smith-rs` is a high-performance system monitoring daemon for Linux, built from the ground up in Rust. 
Just like its namesake, it is designed to be everywhere, monitor everything, and keep a vigilant eye 
on your system's anomalies—minus the desire to destroy humanity.

By leveraging an asynchronous architecture powered by `tokio` and bypassing heavy CPU/PID scanning 
during checks, `smith-rs` ensures extremely lightweight metrics collection without introducing 
thread contention or resource degradation on production instances.

---

## Features

* **Multi-Metric Architecture:** Monitors critical host operating characteristics concurrently without global lock friction:
* **Load Average:** Triggers warnings or critical status reports by inspecting peak sustained pressure configurations across 1, 5, and 15-minute scopes.
* **Memory Usage:** Bypasses external subprocesses to interact directly with internal RAM tables safely via asynchronous resource queries.
* **Disk Space Mapping:** Dynamically resolves storage utilization thresholds mapped to designated OS kernel file systems or raw mount arrays.
* **NTP Drift Synchronization:** Regularly calculates clock drift delta values against public standard synchronization server pools.


* **Native Background Isolation (Daemon mode):** Decouples immediately into a standard Unix background process tracking a designated PID lock file.
* **Configurable SMTP Email Alert Pipeline:** Seamlessly builds secure TLS mail requests targeting designated administrative distribution channels whenever statuses cross warning or critical limits.
* **Dynamic Interval Adjustments:** Individual metric routines adapt to personalized collection loops independent of one another.

---

## Installation & Compilation

Ensure you have a recent version of the Rust compiler installed.

1. **Clone the repository:**
```bash
git clone https://github.com/yourusername/smith-rs.git
cd smith-rs

```


2. **Build the binary in release mode:**
```bash
cargo build --release

```


The high-efficiency compiled executable will be located under `./target/release/smith-rs`.

---

## Configuration (`smith-rs.ini`)

`smith-rs` is configured using a standard INI layout. Below is an example configuration file demonstrating available parameters:

```ini
; General runtime parameters
[Setting]
log_file_path = smith-rs.log
pid_file_path = smith-rs.pid

; Enable or disable individual monitoring tasks
loadaverage = true
loadaverage_period = 10

memoryusage = true
memoryusage_period = 15

diskusage = true
diskusage_period = 60

ntpdrift = true
ntpdrift_period = 120

; Set resource warning and critical thresholds
[System]
hostname = localhost
load_average_warning_threshold = 16.0
load_average_critical_threshold = 24.0
disks = /, /System/Volumes/Data
disk_warning_threshold = 90
disk_critical_threshold = 95
memory_warning_threshold = 85.0
memory_critical_threshold = 95.0

; Network Time Protocol settings
[Ntp]
ntp_pool_server = pool.ntp.org
ntp_warning_threshold = 1.0
ntp_critical_threshold = 3.0

; Notification email configuration
[Email]
smtp_server = smtp.example.com
smtp_port = 465
sender_email = sender@example.com
receiver_email = receiver@example.com
smtp_username = user
smtp_password = pass

; Toggle email routing for specific check groups
[Alerts]
load = false
memoryusage = false
diskusage = false
ntpdrift = false

```

---

## Usage

You can launch the binary immediately from the console. 
By default, `smith-rs` searches for a config file named `smith-rs.ini` in the working context if flags are omitted.

```bash
# Run using default settings
./smith-rs

# Run with a custom configuration path
./smith-rs -c /etc/smith-rs/production.ini
./smith-rs --config /etc/smith-rs/production.ini

# Run detached as a persistent Linux daemon background process
./smith-rs -d --config /etc/smith-rs/production.ini
./smith-rs --daemonize --config /etc/smith-rs/production.ini

```

---

## Project Structure

```text
smith-rs/
├── Cargo.toml          # Rust package dependencies and metadata
├── smith-rs.ini        # Default configuration reference template
├── src/
│   ├── main.rs         # Daemon manager and CLI loop initialization
│   ├── config.rs       # INI parser mappings
│   ├── engine.rs       # Async scheduler handling log and alert tasks
│   ├── alert.rs        # SMTP message assembler and delivery logic
│   └── checks/         # Metric collection modules
│       ├── mod.rs      # Traits and outcome enumeration definitions
│       ├── load.rs     # System load average tracking
│       ├── memory.rs   # RAM statistics monitor
│       ├── disk.rs     # Mount point disk space metrics
│       └── ntp.rs      # Network time synchronization checking

```

---

## License

This project is open-source software distributed under the terms of the **Apache License, Version 2.0**. See the `LICENSE` file for full terms and conditions.
