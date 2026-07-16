// ==========================================
// 1. MODULE DECLARATIONS
// ==========================================
// In Rust, 'pub mod' registers a sub-file (or sub-directory) as a module.
// Marking them 'pub' (public) makes their contents accessible outside of this folder.
// For example, 'pub mod disk;' tells Rust to compile 'disk.rs' in this folder.
pub mod disk;
pub mod inodes;
pub mod iops;
pub mod iowait;
pub mod load;
pub mod memory;
pub mod network;
pub mod network_errors;
pub mod ntp;
pub mod swap;

// Bring in structures from our global config and external dependencies
use crate::config::AppConfig; // AppConfig handles our YAML parsing
use std::collections::HashMap; // A standard key-value map implementation

// ==========================================
// 2. CHECK RESULT ENUMERATION
// ==========================================
// An 'enum' (enumeration) in Rust is a type that can represent one of several variants.
// Unlike basic enums in other languages, Rust enums can store data inside their variants!
#[derive(Debug, Clone)]
pub enum CheckResult {
    // Variant 1: A single-value monitoring result (e.g., Load Average check).
    // It holds two named String parameters: 'status' (e.g. "OK") and 'message' (e.g. "1min: 1.20").
    Single { status: String, message: String },

    // Variant 2: A multi-value monitoring result (e.g., checking multiple disk mounts or network cards).
    // It maps a sub-item name (e.g., "sda" or "/mnt") to a tuple of (Status, Message).
    Multi(HashMap<String, (String, String)>),
}

// ==========================================
// 3. BASE CHECK TRAIT DEFINITION
// ==========================================
// A 'trait' in Rust is similar to an "interface" in Java or C#.
// It defines a contract of methods that a struct must implement to be used by the engine.
//
// Attribute explanation:
// #[async_trait::async_trait] is used because native Rust traits do not natively
// support 'async fn' declarations yet. This macro rewrites the functions safely.
#[async_trait::async_trait]
pub trait BaseCheck: Send + Sync {
    /// Returns a human-readable name of the check (e.g., "Disk Space" or "Load Average").
    fn name(&self) -> &'static str;

    /// Returns the unique key corresponding to this service check in the YAML config (e.g., "disk" or "load").
    fn config_key(&self) -> &'static str;

    /// Returns a default period interval (in seconds) to run this check if none is specified in the YAML config
    fn default_period(&self) -> u64;

    /// Runs the actual monitoring logic asynchronously.
    /// - It receives a reference to the global `AppConfig`.
    /// - It returns an `Option<CheckResult>`:
    ///   - `Some(CheckResult)` if the run succeeded and has state information.
    ///   - `None` if the check shouldn't run or is completely disabled on this platform.
    async fn run(&self, config: &AppConfig) -> Option<CheckResult>;
}

// Note on "BaseCheck: Send + Sync":
// - 'Send' tells the compiler that it is safe to transfer the ownership of objects implementing
//   this trait across different CPU threads.
// - 'Sync' tells the compiler that multiple threads are allowed to safely read this object
//   simultaneously via shared references.
// Since our SmithEngine runs checks concurrently in background worker threads, both bounds are strictly required!
