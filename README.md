# smith-rs

`smith-rs` is a high-performance system monitoring daemon for Linux, built from the ground up in Rust. 
Just like its namesake, it is designed to be everywhere, monitor everything, and keep a vigilant eye 
on your system's anomalies—minus the desire to destroy humanity.

By leveraging an asynchronous architecture powered by `tokio` and bypassing heavy CPU/PID scanning 
during checks, `smith-rs` ensures extremely lightweight metrics collection without introducing 
thread contention or resource degradation on production instances.

---

## Features

*   **Nagios Core-Style State Machine**: Prevents flapping and false positives by tracking consecutive failures across **SOFT** and **HARD** states.
*   **YAML-Based Object Configuration**: Modern, human-readable object definitions replacing restrictive INI layouts while preventing key duplication issues.
*   **Timeperiods Management**: Restricts check executions and alert dispatches based on customizable weekly schedules (e.g., `24x7`, `work_hours`).
*   **Multi-Contact Routing**: Dispatches alerts to multiple distinct email recipients asynchronously, filtered dynamically by their working hours and notification preferences (`w,u,c,r`).
*   **Built-In Performance Plugins**:
    *   **Load**: 1, 5, and 15-minute system load averages.
    *   **Memory**: RAM utilization percentages.
    *   **Disk**: Multi-mount point storage space validation.
    *   **NTP**: Clock drift measurements against an external NTP pool server.
*   **Daemonization**: Built-in support to safely detach, fork, and run as a low-overhead system background daemon[cite: 1].

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


## Configuration (config.yaml)
The agent uses a single structured config.yaml file to define settings, time schedules, contacts, and monitoring thresholds:

```yaml
setting:
  log_file_path: "smith-rs.log"
  pid_file_path: "smith-rs.pid"
  debug: false

system:
  hostname: "localhost"

email:
  smtp_server: "smtp.example.com"
  smtp_port: 465
  sender_email: "alerts@example.com"
  smtp_username: "user"
  smtp_password: "pass"

timeperiods:
  - name: "24x7"
    alias: "24 Hours A Day, 7 Days A Week"
    sunday: "00:00-24:00"
    monday: "00:00-24:00"
    tuesday: "00:00-24:00"
    wednesday: "00:00-24:00"
    thursday: "00:00-24:00"
    friday: "00:00-24:00"
    saturday: "00:00-24:00"

  - name: "work_hours"
    alias: "Work Hours"
    monday: "09:00-17:00"
    tuesday: "09:00-17:00"
    wednesday: "09:00-17:00"
    thursday: "09:00-17:00"
    friday: "09:00-17:00"

contacts:
  - name: "john_smith"
    alias: "John Smith"
    email: "john.smiths@matrix.gov"
    notification_period: "work_hours"
    notification_options: ["w", "u", "c", "r"] # warning, unknown, critical, recovery

services:
  - name: "load"
    description: "Load Average"
    active: true
    check_interval: 10
    check_attempts: 3
    check_time_period: "24x7"
    warning: 16.0
    critical: 24.0

  - name: "memory"
    description: "Memory Usage"
    active: true
    check_interval: 15
    check_attempts: 3
    check_time_period: "24x7"
    warning: 85.0
    critical: 95.0

  - name: "disk"
    description: "Disk Space"
    active: true
    check_interval: 60
    check_attempts: 2
    check_time_period: "24x7"
    warning: 90.0
    critical: 95.0
    disks: ["/", "/var"]

  - name: "ntp"
    description: "NTP Drift"
    active: true
    check_interval: 120
    check_attempts: 3
    check_time_period: "24x7"
    warning: 1.0
    critical: 3.0
    ntp_pool_server: "pool.ntp.org"

```

---

## State Machine & Notification Logic

Soft vs. Hard States
When a service check fails for the first time, it enters a SOFT alert state. No email notification is sent yet.

The agent schedules a re-check based on the service interval.

If the service check fails consistently up to check_attempts consecutive times, the state officially transitions into a HARD state.

An email alert is immediately triggered and dispatched to all contacts whose operational timeperiod and flags match.

If an unhealthy service returns to nominal parameters, it immediately transitions to a HARD OK state and fires a recovery (r) notification.

---

## Usage

You can launch the binary immediately from the console. 

```bash
# Run with a custom configuration path
./smith-rs -c /etc/smith-rs/production.yaml
./smith-rs --config /etc/smith-rs/production.yaml

# Run detached as a persistent Linux daemon background process
./smith-rs -d --config /etc/smith-rs/production.yaml
./smith-rs --daemonize --config /etc/smith-rs/production.yaml

```

When daemonized, standard output and errors are redirected to the configured **log_file_path**, 
and the running process ID is written to **pid_file_path**.

---

## Project Structure

```text
smith-rs/
├── Cargo.toml          # Rust package dependencies and metadata
├── smith-rs.yaml        # Default configuration reference template
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

This project is open-source software distributed under the terms of the 
**Apache License, Version 2.0**. See the `LICENSE` file for full terms and conditions.

