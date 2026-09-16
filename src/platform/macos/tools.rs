//! Locates the helper binaries shipped inside the `.app` bundle.
//!
//! Mac2Win Go is meant to work out of the box, so `wimlib-imagex`, `mkntfs`,
//! `ms-sys` and the BCD-SYS helper are bundled under `Contents/Resources/tools`.
//! During development they can live in `vendor/tools`, and finally we fall back
//! to `PATH` (Homebrew, MacPorts, ...).

use std::path::{Path, PathBuf};

use crate::core::error::{CoreError, Result};

/// A bundled helper the writer may need.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Tool {
    /// `wimlib-imagex` built with `libntfs-3g` (applies WIMs to raw NTFS).
    WimlibImagex,
    /// `mkntfs` from ntfs-3g (creates NTFS without macFUSE).
    Mkntfs,
    /// `ntfscat` reads a file from an unmounted NTFS partition.
    NtfsCat,
    /// `ntfscp` writes a file to an unmounted NTFS partition.
    NtfsCp,
    /// `hiveset` sets a single value in an offline registry hive.
    HiveSet,
    /// `hivexsh` edits the BCD hive (used by BCD-SYS).
    HivexSh,
    /// `ms-sys` (legacy BIOS boot code).
    MsSys,
    /// BCD-SYS helper (copies boot files and generates the BCD store).
    BcdSys,
}

impl Tool {
    /// All tools required for the default UEFI path (BIOS-only tools excluded).
    pub const UEFI: [Tool; 7] = [
        Tool::WimlibImagex,
        Tool::Mkntfs,
        Tool::NtfsCat,
        Tool::NtfsCp,
        Tool::HiveSet,
        Tool::HivexSh,
        Tool::BcdSys,
    ];

    /// All tools, including legacy BIOS support.
    pub const ALL: [Tool; 8] = [
        Tool::WimlibImagex,
        Tool::Mkntfs,
        Tool::NtfsCat,
        Tool::NtfsCp,
        Tool::HiveSet,
        Tool::HivexSh,
        Tool::BcdSys,
        Tool::MsSys,
    ];

    /// Executable file name inside a tools directory.
    #[must_use]
    pub fn binary_name(self) -> &'static str {
        match self {
            Self::WimlibImagex => "wimlib-imagex",
            Self::Mkntfs => "mkntfs",
            Self::NtfsCat => "ntfscat",
            Self::NtfsCp => "ntfscp",
            Self::HiveSet => "hiveset",
            Self::HivexSh => "hivexsh",
            Self::MsSys => "ms-sys",
            Self::BcdSys => "bcd-sys",
        }
    }
}

/// Resolves helper executables from bundled directories or `PATH`.
#[derive(Debug, Clone, Default)]
pub struct ToolLocator {
    search_dirs: Vec<PathBuf>,
    path_env: Option<String>,
}

impl ToolLocator {
    /// Locator for the running application: bundle, dev tree, then `PATH`.
    #[must_use]
    pub fn system() -> Self {
        let mut search_dirs = Vec::new();

        if let Ok(dir) = std::env::var("MAC2WINGO_TOOLS") {
            if !dir.is_empty() {
                search_dirs.push(PathBuf::from(dir));
            }
        }
        if let Some(dir) = bundle_tools_dir() {
            search_dirs.push(dir);
        }
        if let Some(dir) = dev_tools_dir() {
            search_dirs.push(dir);
        }
        search_dirs.push(PathBuf::from("/opt/homebrew/bin"));
        search_dirs.push(PathBuf::from("/usr/local/bin"));

        Self {
            search_dirs,
            path_env: std::env::var("PATH").ok(),
        }
    }

    /// Locator with an explicit set of search directories (used in tests).
    #[must_use]
    pub fn with_search_dirs(search_dirs: Vec<PathBuf>) -> Self {
        Self {
            search_dirs,
            path_env: None,
        }
    }

    /// Resolve a tool to an absolute path, if present.
    #[must_use]
    pub fn locate(&self, tool: Tool) -> Option<PathBuf> {
        let name = tool.binary_name();
        for dir in &self.search_dirs {
            let candidate = dir.join(name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
        if let Some(path) = &self.path_env {
            for dir in path.split(':').filter(|entry| !entry.is_empty()) {
                let candidate = Path::new(dir).join(name);
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
        None
    }

    /// Resolve a tool or fail with [`CoreError::ToolMissing`].
    pub fn require(&self, tool: Tool) -> Result<PathBuf> {
        self.locate(tool)
            .ok_or_else(|| CoreError::ToolMissing(tool.binary_name().to_owned()))
    }

    /// Tools from `required` that could not be found.
    #[must_use]
    pub fn missing(&self, required: &[Tool]) -> Vec<Tool> {
        required
            .iter()
            .copied()
            .filter(|tool| self.locate(*tool).is_none())
            .collect()
    }

    /// Fail if any of `required` is missing.
    pub fn preflight(&self, required: &[Tool]) -> Result<()> {
        if let Some(tool) = self.missing(required).into_iter().next() {
            return Err(CoreError::ToolMissing(tool.binary_name().to_owned()));
        }
        Ok(())
    }
}

/// `<exe>/../Resources/tools` inside the `.app` bundle.
#[must_use]
pub fn bundle_tools_dir() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let macos_dir = exe.parent()?;
    let contents_dir = macos_dir.parent()?;
    Some(contents_dir.join("Resources").join("tools"))
}

/// `vendor/tools` in the source tree, for `cargo run`.
#[must_use]
pub fn dev_tools_dir() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    // target/<profile>/<exe> -> repository root.
    let root = exe.parent()?.parent()?.parent()?;
    Some(root.join("vendor").join("tools"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_first_matching_directory() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("mkntfs"), b"#!/bin/sh\n").unwrap();

        let locator = ToolLocator::with_search_dirs(vec![dir.path().to_path_buf()]);
        assert!(locator.locate(Tool::Mkntfs).is_some());
        assert!(locator.locate(Tool::WimlibImagex).is_none());
    }

    #[test]
    fn require_reports_missing_tool() {
        let locator = ToolLocator::with_search_dirs(vec![]);
        let error = locator.require(Tool::WimlibImagex).unwrap_err();
        assert!(matches!(error, CoreError::ToolMissing(name) if name == "wimlib-imagex"));
    }

    #[test]
    fn preflight_lists_missing_tools() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("wimlib-imagex"), b"x").unwrap();
        std::fs::write(dir.path().join("mkntfs"), b"x").unwrap();

        let locator = ToolLocator::with_search_dirs(vec![dir.path().to_path_buf()]);
        assert_eq!(
            locator.missing(&Tool::UEFI),
            vec![
                Tool::NtfsCat,
                Tool::NtfsCp,
                Tool::HiveSet,
                Tool::HivexSh,
                Tool::BcdSys
            ]
        );
        assert!(locator.preflight(&Tool::UEFI).is_err());
    }
}
