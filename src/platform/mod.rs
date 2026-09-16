//! Platform specific integrations. Currently only macOS is implemented.

#[cfg(target_os = "macos")]
pub mod macos;

#[cfg(target_os = "macos")]
pub use macos::diskutil::DiskutilDeviceService;

pub use crate::core::command::SystemCommandRunner;
