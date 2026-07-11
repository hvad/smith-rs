use crate::checks::{BaseCheck, CheckResult};
use crate::config::AppConfig;

pub struct SwapUsageCheck;

impl SwapUsageCheck {
    #[cfg(target_os = "linux")]
    fn get_swap_percentage(&self) -> Result<Option<f64>, String> {
        use std::fs::File;
        use std::io::{BufRead, BufReader};

        let file = File::open("/proc/meminfo").map_err(|e| e.to_string())?;
        let reader = BufReader::new(file);

        let mut swap_total: Option<u64> = None;
        let mut swap_free: Option<u64> = None;

        for line in reader.lines().map_while(Result::ok) {
            if line.starts_with("SwapTotal:") {
                swap_total = line.split_whitespace().nth(1).and_then(|s| s.parse().ok());
            } else if line.starts_with("SwapFree:") {
                swap_free = line.split_whitespace().nth(1).and_then(|s| s.parse().ok());
            }
            if swap_total.is_some() && swap_free.is_some() {
                break;
            }
        }

        match (swap_total, swap_free) {
            (Some(total), Some(free)) if total > 0 => {
                let used = total.saturating_sub(free);
                Ok(Some((used as f64 / total as f64) * 100.0))
            }
            (Some(0), _) | (None, None) => Ok(None), // Swap is completely disabled/unallocated
            _ => Err("Failed to accurately parse /proc/meminfo swap keys".to_string()),
        }
    }

    #[cfg(target_os = "macos")]
    fn get_swap_percentage(&self) -> Result<Option<f64>, String> {
        use std::ffi::CString;
        use std::mem;
        use std::ptr;

        // Native Mach structure definition matching <sys/sysctl.h> vm_swapusage
        #[repr(C)]
        struct VmSwapusage {
            xsu_total: u64,
            xsu_avail: u64,
            xsu_used: u64,
            xsu_pagesize: u32,
            xsu_encrypted: i32,
        }

        let name = CString::new("vm.swapusage").map_err(|e| e.to_string())?;
        let mut size = mem::size_of::<VmSwapusage>();
        let mut swapusage: VmSwapusage = unsafe { mem::zeroed() };

        let result = unsafe {
            libc::sysctlbyname(
                name.as_ptr(),
                &mut swapusage as *mut _ as *mut libc::c_void,
                &mut size,
                ptr::null_mut(),
                0,
            )
        };

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
}

#[async_trait::async_trait]
impl BaseCheck for SwapUsageCheck {
    fn name(&self) -> &'static str {
        "Swap Utilization"
    }

    fn config_key(&self) -> &'static str {
        "swap"
    }

    fn default_period(&self) -> u64 {
        30
    }

    async fn run(&self, config: &AppConfig) -> Option<CheckResult> {
        let (warn, crit) = if let Some(sc) = config.services.get(self.config_key()) {
            (sc.warning, sc.critical)
        } else {
            (40.0, 70.0)
        };

        match self.get_swap_percentage() {
            Ok(Some(percent)) => {
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
