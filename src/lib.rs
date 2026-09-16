//! Mac2Win Go — create bootable Windows To Go drives on macOS.
//!
//! The crate is split into a platform-independent [`core`] domain, platform
//! integrations under [`platform`], service implementations under [`services`]
//! and the egui user interface under [`ui`].

#![forbid(unsafe_code)]

pub mod app;
pub mod core;
pub mod i18n;
pub mod platform;
pub mod services;
pub mod ui;

pub use app::{Mac2WinApp, Services, Step};
pub use core::AppConfig;
pub use i18n::{I18n, Language};
