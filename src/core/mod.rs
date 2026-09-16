//! Core domain types and business-logic contracts.
//!
//! Nothing in this module may depend on the UI toolkit, so that the domain can
//! be unit-tested in isolation and reused from a future CLI.

pub mod command;
pub mod config;
pub mod device;
pub mod drivers;
pub mod error;
pub mod format;
pub mod image;
pub mod options;
pub mod progress;
pub mod writer;

pub use command::{Command, CommandRunner, SystemCommandRunner};
pub use config::AppConfig;
pub use device::{BusType, DeviceService, DiskDevice, Volume};
pub use drivers::{DriverSource, SteamHardware};
pub use error::{CoreError, Result};
pub use image::{ImageFormat, ImageService, WindowsImage};
pub use options::{FileSystem, PartitionScheme, WriteOptions};
pub use progress::{ProgressEvent, ProgressSink, Stage};
pub use writer::{WindowsToGoWriter, WritePlan};
