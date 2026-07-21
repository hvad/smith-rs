// Bring in the BaseCheck trait and CheckResult enum from our checks module
use crate::checks::{BaseCheck, CheckResult};
// Bring in the resolved global configurations
use crate::config::AppConfig;

/// The main struct responsible for evaluating system Swap space utilization
pub struct SwapUsageCheck;

impl SwapUsageCheck {
    // ==========================================
    // 1. LINUX SPECIFIC IMPLEMENTATION
    // ==========================================
    /// Parses `/proc/meminfo` to extract `SwapTotal` and `SwapFree` values on Linux.
    /// Conditional compilation attribute ensures this block compiles ONLY on Linux targets.
    #[cfg(target_os = "linux")]
    fn get_swap_percentage(&self) -> Result<Option<f64>, String> {
        use std::fs::File;
        use std::io::{BufRead, BufReader};

        // Open the virtual proc filesystem file containing kernel memory statistics
        let file = File::open("/proc/meminfo").map_err(|e| e.to_string())?;
        let reader = BufReader::new(file);

        let mut swap_total: Option<u64> = None;
        let mut swap_free: Option<u64> = None;

        // Read through lines sequentially until both SwapTotal and SwapFree are parsed
        for line in reader.lines().map_while(Result::ok) {
            if line.starts_with("SwapTotal:") {
                // Split string by whitespace and parse the second token (kB value)
                swap_total = line.split_whitespace().nth(1).and_then(|s| s.parse().ok());
            } else if line.starts_with("SwapFree:") {
                swap_free = line.split_whitespace().nth(1).and_then(|s| s.parse().ok());
            }

            // Stop reading early once both metrics are found
            if swap_total.is_some() && swap_free.is_some() {
                break;
            }
        }

        // Match on the extracted pair to evaluate percentage
        match (swap_total, swap_free) {
            (Some(total), Some(free)) if total > 0 => {
                // Prevent arithmetic underflow if free unexpectedly exceeds total
                let used = total.saturating_sub(free);
                Ok(Some((used as f64 / total as f64) * 100.0))
            }
            // If total swap is 0 or unassigned, treat it as inactive/unallocated swap space
            (Some(0), _) | (None, None) => Ok(None),
            _ => Err("Failed to accurately parse /proc/meminfo swap keys".to_string()),
        }
    }

    // ==========================================
    // 2. MACOS (OSX) SPECIFIC IMPLEMENTATION
    // ==========================================
    /// Calls the macOS C Kernel sysctl function (`vm.swapusage`) to query swap statistics.
    /// Conditional compilation attribute ensures this block compiles ONLY on macOS targets.
    #[cfg(target_os = "macos")]
    fn get_swap_percentage(&self) -> Result<Option<f64>, String> {
        use std::ffi::CString;
        use std::mem;
        use std::ptr;

        // C-compatible memory layout matching macOS <sys/sysctl.h> struct vm_swapusage
        #[repr(C)]
        struct VmSwapusage {
            xsu_total: u64,
            xsu_avail: u64,
            xsu_used: u64,
            xsu_pagesize: u32,
            xsu_encrypted: i32,
        }

        // Convert key name string into a C-compatible null-terminated string pointer
        let name = CString::new("vm.swapusage").map_err(|e| e.to_string())?;
        let mut size = mem::size_of::<VmSwapusage>();

        // Unsafely instantiate a zero-initialized memory block for the struct
        let mut swapusage: VmSwapusage = unsafe { mem::zeroed() };

        // Make FFI system call to sysctlbyname
        let result = unsafe {
            libc::sysctlbyname(
                name.as_ptr(),
                &mut swapusage as *mut _ as *mut libc::c_void,
                &mut size,
                ptr::null_mut(),
                0,
            )
        };

        // sysctlbyname returns 0 on successful execution
        if result == 0 {
            if swapusage.xsu_total == 0 {
                return Ok(None);
            }
            let percentage = (swapusage.xsu_used as f64 / swapusage.xsu_total as f64) * 100.0;
            Ok(Some(percentage))
        } else {
            Err("sysctlbyname returned error executing vm.swapusage sequence".to_string())
        }
    }

    // ==========================================
    // 3. FALLBACK FOR OTHER PLATFORMS
    // ==========================================
    /// Fallback implementation if compiled on unsupported targets (e.g., Windows or BSD).
    #[cfg(all(not(target_os = "linux"), not(target_os = "macos")))]
    fn get_swap_percentage(&self) -> Result<Option<f64>, String> {
        Err("Swap check is not supported on this operating system".to_string())
    }
}

// Implement the BaseCheck trait asynchronously
#[async_trait::async_trait]
impl BaseCheck for SwapUsageCheck {
    /// Returns the human-readable name of this check
    fn name(&self) -> &'static str {
        "Swap Utilization"
    }

    /// Returns the configuration key used in the YAML file to customize this service
    fn config_key(&self) -> &'static str {
        "swap"
    }

    /// Default execution interval (30 seconds) if not overridden in the configuration
    fn default_period(&self) -> u64 {
        30
    }

    /// Asynchronously checks the Swap utilization status across OS implementations
    async fn run(&self, config: &AppConfig) -> Option<CheckResult> {
        // Resolve warning and critical threshold values from configuration or defaults
        let (warn, crit) = if let Some(sc) = config.services.get(self.config_key()) {
            (sc.warning, sc.critical)
        } else {
            (40.0, 70.0) // Defaults: 40% Warning, 70% Critical
        };

        // Query platform-specific helper method
        match self.get_swap_percentage() {
            Ok(Some(percent)) => {
                // Evaluate metric against configured thresholds
                let status = if percent >= crit {
                    "CRITICAL".to_string()
                } else if percent >= warn {
                    "WARNING".to_string()
                } else {
                    "OK".to_string()
                };

                Some(CheckResult::Single {
                    status,
                    message: format!("Swap utilization: {:.2}%", percent),
                })
            }
            Ok(None) => Some(CheckResult::Single {
                status: "OK".to_string(),
                message: "Swap space is not active or unallocated on this system".to_string(),
            }),
            Err(err) => Some(CheckResult::Single {
                status: "UNKNOWN".to_string(),
                message: err,
            }),
        }
    }
}
