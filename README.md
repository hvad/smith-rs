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
    *   **Network**: Monitors interfaces for high traffic load, packet drops, or transmission errors before your network turns into a bottleneck..
    *   **NTP**: Clock drift measurements against an external NTP pool server.
*   **Daemonization**: Built-in support to safely detach, fork, and run as a low-overhead system background daemon.

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
The agent uses a single structured config.yaml file to define settings, time schedules, contacts,
and monitoring thresholds:

```yaml
# ==============================================================================
# GLOBAL SETTINGS & ENVIRONMENT ENGINE CONFIGURATION
# ==============================================================================
setting:
  log_file_path: "smith-agent.log"
  pid_file_path: "smith-agent.pid"
  debug: false

system:
  hostname: "localhost"

# ==============================================================================
# EMAIL NOTIFICATION (SMTP) GATEWAY SETUP
# ==============================================================================
email:
  smtp_server: "smtp.mailtrap.io"
  smtp_port: 587
  sender_email: "smith-agent@matrix.gov"
  # SMTP credentials can be omitted or commented out if your server doesn't require authentication
  smtp_username: "matrix_operator"
  smtp_password: "neon_sky_password"

# ==============================================================================
# TIMEPERIOD BLUEPRINTS AND INSTANCES (Nagios-like Inheritence)
# ==============================================================================
timeperiods:
  # 1. Base Blueprint (Template)
  - name: "base_24x7"
    register: false # Declares this object as a template blueprint
    alias: "Standard Always-On 24x7 Frame"
    sunday: "00:00-24:00"
    monday: "00:00-24:00"
    tuesday: "00:00-24:00"
    wednesday: "00:00-24:00"
    thursday: "00:00-24:00"
    friday: "00:00-24:00"
    saturday: "00:00-24:00"

  # 2. Final Enrolled Instances
  - name: "24x7"
    use_template: "base_24x7" # Inherits all day windows from base_24x7
    alias: "Production standard non-stop"

  - name: "work_hours"
    alias: "Standard Business Work Hours"
    sunday: "00:00-00:00" # Disabled
    monday: "09:00-17:00"
    tuesday: "09:00-17:00"
    wednesday: "09:00-17:00"
    thursday: "09:00-17:00"
    friday: "09:00-17:00"
    saturday: "00:00-00:00" # Disabled

# ==============================================================================
# CONTACT BLUEPRINTS AND INSTANCES
# ==============================================================================
contacts:
  # 1. Base Blueprint (Template)
  - name: "generic_admin"
    register: false # Declares this object as a template blueprint
    alias: "Generic Administrator Placeholder"
    notification_period: "24x7"
    notification_options: ["w", "c", "r"] # warning, critical, recovery

  - name: "oncall_team"
    use_template: "generic_admin"
    alias: "Systems On-Call Duty Team"
    email: "oncall-alerts@matrix.gov"
    notification_period: "work_hours" # Overrides period to business hours only

# ==============================================================================
# SERVICE CHECKS BLUEPRINTS AND INSTANCES
# ==============================================================================
services:
  # ----------------------------------------------------------------------------
  # SERVICE TEMPLATES (Blueprints - register: false)
  # ----------------------------------------------------------------------------
  - name: "generic_service"
    register: false
    active: true
    check_interval: 15          # Check every 15 seconds
    check_attempts: 3           # Max attempts before transition from SOFT to HARD alert
    check_time_period: "24x7"
    warning: 80.0
    critical: 90.0

  - name: "generic_network_service"
    register: false
    active: true
    check_interval: 10
    check_attempts: 2
    check_time_period: "24x7"
    warning: 100.0
    critical: 500.0

  # ----------------------------------------------------------------------------
  # ACTIVE MONITORING INSTANCES (Enrolled - register: true implicitly)
  # ----------------------------------------------------------------------------
  
  # SYSTEM LOAD AVERAGE
  - name: "load"
    use_template: "generic_service"
    description: "Load Average"
    check_interval: 10
    warning: 16.0   # Overrides standard 80.0 to match CPU core counts
    critical: 24.0  # Overrides standard 90.0

  # KERNEL DELAY METRIC (CPU Wait Time on IO Operations)
  - name: "iowait"
    use_template: "generic_service"
    description: "I/O Wait"
    warning: 15.0 # Warn if CPU spends >15% time waiting on disks
    critical: 30.0

  # RAM MEMORY UTILIZATION
  - name: "memory"
    use_template: "generic_service"
    description: "Memory Usage"
    warning: 85.0   # Alert if RAM usage passes 85%
    critical: 95.0  # Alert if RAM usage passes 95%

  # SWAP SPACE MEMORY UTILIZATION
  - name: "swap"
    use_template: "generic_service"
    description: "Swap Utilization"
    warning: 40.0
    critical: 70.0

  # FILESYSTEM STORAGE METRICS
  - name: "disk"
    use_template: "generic_service"
    description: "Disk Space"
    check_interval: 60 # Disks change slower; check once a minute
    warning: 90.0
    critical: 95.0
    disks: ["/", "/var"]

  # SYSTEM FILE INODES DENSITY
  - name: "inodes"
    use_template: "generic_service"
    description: "Inode Utilization"
    check_interval: 60
    warning: 80.0
    critical: 90.0
    disks: ["/"]

  # HARDWARE STORAGE CONTROLLER IOPS SPEED RATE
  - name: "iops"
    use_template: "generic_service"
    active: false
    description: "Disk IOPS"
    warning: 1000.0   # Warning threshold rate in operations/sec
    critical: 2000.0  # Critical threshold rate in operations/sec
    disks: ["vda"]

  # NETWORK INTERFACE CARD BANDWIDTH THROUGHPUT
  - name: "network"
    use_template: "generic_network_service"
    description: "Network Throughput"
    warning: 600.0    # Combined RX/TX rate limit warning at 600 Mbps
    critical: 900.0   # Combined RX/TX rate limit critical at 900 Mbps
    interfaces: ["eth0"] # Use array configuration for card names

  # NETWORK HARDWARE ERROR/DROP PACKETS DISCARD RATE
  - name: "network_errors"
    use_template: "generic_network_service"
    description: "Network Errors"
    warning: 0.05   # Warn if packet drop burst rate climbs over 0.05 drops/sec
    critical: 1.0   # Critical state if dropping >= 1 frame every single second
    interfaces: ["eth0"] # Use array configuration for card names

  # CORE NETWORK SOCK STATS (PORT EXHAUSTION DEFENSE)
  - name: "tcp_states"
    use_template: "generic_network_service"
    description: "TCP Connection States"
    warning: 400.0  # Alert if combined active monitored sockets pass 400
    critical: 800.0 # Port depletion risk threshold at 800 sockets

  # TIME PROTOCOL ACCURACY (NTP CLOCK DRIFT)
  - name: "ntp"
    use_template: "generic_service"
    description: "NTP Drift"
    check_interval: 600 # Check once every two minutes
    warning: 0.005      # Warn if system clock drifts more than 5 milliseconds
    critical: 0.05      # Critical alert if clock drifts more than 50 milliseconds
    ntp_pool_server: "pool.ntp.org"
```

---

## State Machine & Notification Logic

**Soft vs. Hard States**

When a service check fails for the first time, it enters a SOFT alert state. No email notification
is sent yet.

The agent schedules a re-check based on the service interval.

If the service check fails consistently up to check_attempts consecutive times, the state officially
transitions into a HARD state.

An email alert is immediately triggered and dispatched to all contacts whose operational timeperiod
and flags match.

If an unhealthy service returns to nominal parameters, it immediately transitions to a HARD OK state
and fires a recovery (r) notification.

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

