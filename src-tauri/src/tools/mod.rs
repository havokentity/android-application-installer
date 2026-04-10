//! Managed tools: downloads ADB platform-tools, bundletool, and a JRE into the
//! app's local data directory so the user never needs to install the Android SDK.

pub(crate) mod config;
pub(crate) mod download;
pub(crate) mod paths;
pub(crate) mod recent;
pub(crate) mod status;

// Re-export path helpers used by other crate modules (adb, java, etc.)
pub use paths::{get_data_dir, managed_adb_path, managed_bundletool_path, managed_java_path};
