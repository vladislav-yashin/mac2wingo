//! Real [`ImageService`] backed by `wimlib-imagex` and `hdiutil`.
//!
//! For `.wim`/`.esd` files the metadata is read directly with
//! `wimlib-imagex info --xml`. For `.iso` images the ISO is mounted read-only
//! and the contained `sources/install.wim`/`install.esd` is inspected.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::core::command::{Command, CommandRunner, SystemCommandRunner};
use crate::core::error::{CoreError, Result};
use crate::core::image::{
    parse_wim_info_xml, validate_extension, ImageEdition, ImageFormat, ImageService, WindowsImage,
};
use crate::platform::macos::tools::{Tool, ToolLocator};

const HDIUTIL: &str = "/usr/bin/hdiutil";

/// A read-only ISO mount that is detached when dropped.
pub struct IsoMount {
    runner: Arc<dyn CommandRunner>,
    mount_point: PathBuf,
    _dir: tempfile::TempDir,
}

impl IsoMount {
    /// Attach `iso` read-only under a fresh temporary mount point.
    pub fn mount(runner: Arc<dyn CommandRunner>, iso: &Path) -> Result<Self> {
        let dir = tempfile::TempDir::new().map_err(|error| CoreError::Mount(error.to_string()))?;
        let mount_point = dir.path().join("iso");
        std::fs::create_dir_all(&mount_point)
            .map_err(|error| CoreError::Mount(error.to_string()))?;

        runner.run(&Command::new(HDIUTIL).args([
            "attach".to_owned(),
            "-nobrowse".to_owned(),
            "-readonly".to_owned(),
            "-mountpoint".to_owned(),
            mount_point.display().to_string(),
            iso.display().to_string(),
        ]))?;

        Ok(Self {
            runner,
            mount_point,
            _dir: dir,
        })
    }

    /// Path to the contained install image.
    pub fn install_image(&self) -> Result<PathBuf> {
        find_install_image(&self.mount_point).ok_or_else(|| {
            CoreError::UnsupportedImage(format!(
                "no sources/install.wim or install.esd inside {}",
                self.mount_point.display()
            ))
        })
    }
}

impl Drop for IsoMount {
    fn drop(&mut self) {
        let _ = self.runner.run(
            &Command::new(HDIUTIL)
                .args(["detach".to_owned(), self.mount_point.display().to_string()]),
        );
    }
}

/// [`ImageService`] that reads real WIM metadata.
pub struct WimImageService {
    runner: Arc<dyn CommandRunner>,
    tools: ToolLocator,
}

impl WimImageService {
    /// Production service using the system tools and bundled helpers.
    #[must_use]
    pub fn system() -> Self {
        Self {
            runner: Arc::new(SystemCommandRunner),
            tools: ToolLocator::system(),
        }
    }

    /// Service with injected dependencies (used in tests).
    #[must_use]
    pub fn with_runner(runner: Arc<dyn CommandRunner>, tools: ToolLocator) -> Self {
        Self { runner, tools }
    }

    /// Read the editions from a WIM/ESD container.
    pub fn editions(&self, wim: &Path) -> Result<Vec<ImageEdition>> {
        let wimlib = self.tools.require(Tool::WimlibImagex)?;
        let output = self
            .runner
            .run(&Command::new(wimlib.display().to_string()).args([
                "info".to_owned(),
                "--xml".to_owned(),
                wim.display().to_string(),
            ]))?;
        parse_wim_info_xml(&output)
    }

    /// Resolve the concrete WIM/ESD container, mounting the ISO when needed.
    pub fn resolve_container(&self, image: &WindowsImage) -> Result<IsoMountOrPath> {
        match image.format {
            ImageFormat::Iso => {
                let mount = IsoMount::mount(Arc::clone(&self.runner), &image.path)?;
                let install = mount.install_image()?;
                Ok(IsoMountOrPath {
                    path: install,
                    _mount: Some(mount),
                })
            }
            ImageFormat::Wim | ImageFormat::Esd => Ok(IsoMountOrPath {
                path: image.path.clone(),
                _mount: None,
            }),
            ImageFormat::Unknown => Err(CoreError::UnsupportedImage(
                image.path.display().to_string(),
            )),
        }
    }
}

impl Default for WimImageService {
    fn default() -> Self {
        Self::system()
    }
}

impl ImageService for WimImageService {
    fn inspect(&self, path: &Path) -> Result<WindowsImage> {
        if !path.exists() {
            return Err(CoreError::ImageNotFound(path.display().to_string()));
        }

        let format = validate_extension(path)?;
        let size_bytes = std::fs::metadata(path)?.len();
        let mut image = WindowsImage {
            path: path.to_path_buf(),
            format,
            size_bytes,
            edition: None,
            architecture: None,
            edition_index: 0,
            version: None,
            editions: Vec::new(),
        };

        let container = self.resolve_container(&image)?;
        image.editions = self.editions(&container.path)?;

        if let Some(first) = image.editions.first().cloned() {
            image = image.with_edition(first.index);
        }

        Ok(image)
    }
}

/// A resolved WIM/ESD path plus an optional ISO mount that keeps it alive.
pub struct IsoMountOrPath {
    pub path: PathBuf,
    _mount: Option<IsoMount>,
}

/// Locate `sources/install.wim` or `sources/install.esd` under a mount point.
#[must_use]
pub fn find_install_image(root: &Path) -> Option<PathBuf> {
    let sources = root.join("sources");
    for name in ["install.wim", "install.esd"] {
        let candidate = sources.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    const WIM_XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<WIM>
    <TOTALBYTES>6000000000</TOTALBYTES>
    <IMAGE INDEX="1">
        <NAME>Windows 11 Home</NAME>
        <DESCRIPTION>Windows 11 Home</DESCRIPTION>
        <TOTALBYTES>10000000000</TOTALBYTES>
        <WINDOWS>
            <ARCH>9</ARCH>
            <EDITIONID>Core</EDITIONID>
            <VERSION><MAJOR>10</MAJOR><MINOR>0</MINOR><BUILD>22631</BUILD></VERSION>
        </WINDOWS>
    </IMAGE>
    <IMAGE INDEX="2">
        <NAME>Windows 11 Pro</NAME>
        <TOTALBYTES>11000000000</TOTALBYTES>
        <WINDOWS>
            <ARCH>9</ARCH>
            <EDITIONID>Professional</EDITIONID>
            <VERSION><MAJOR>10</MAJOR><MINOR>0</MINOR><BUILD>26100</BUILD></VERSION>
        </WINDOWS>
    </IMAGE>
</WIM>"#;

    #[derive(Default)]
    struct FakeRunner(Mutex<Vec<Command>>);

    impl CommandRunner for FakeRunner {
        fn run(&self, command: &Command) -> Result<String> {
            self.0.lock().unwrap().push(command.clone());
            Ok(WIM_XML.to_owned())
        }
    }

    fn service(dir: &Path) -> WimImageService {
        let locator = ToolLocator::with_search_dirs(vec![dir.to_path_buf()]);
        WimImageService::with_runner(Arc::new(FakeRunner::default()), locator)
    }

    #[test]
    fn inspects_wim_and_selects_first_edition() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("wimlib-imagex"), b"x").unwrap();
        let wim = dir.path().join("install.wim");
        std::fs::write(&wim, b"fake").unwrap();

        let image = service(dir.path()).inspect(&wim).unwrap();
        assert_eq!(image.editions.len(), 2);
        assert_eq!(image.edition_index, 1);
        assert_eq!(image.edition.as_deref(), Some("Windows 11 Home"));
        assert_eq!(image.architecture.as_deref(), Some("x86_64"));
        assert_eq!(image.editions[1].size_bytes, Some(11_000_000_000));
        assert_eq!(image.version.unwrap().build, 22631);
        assert!(!image.is_wtg_unsupported());
        assert!(image.with_edition(2).is_wtg_unsupported());
    }

    #[test]
    fn missing_wimlib_is_reported() {
        let dir = tempfile::tempdir().unwrap();
        let wim = dir.path().join("install.wim");
        std::fs::write(&wim, b"fake").unwrap();

        let error = service(dir.path()).inspect(&wim).unwrap_err();
        assert!(matches!(error, CoreError::ToolMissing(_)));
    }

    #[test]
    fn finds_install_image_in_sources() {
        let dir = tempfile::tempdir().unwrap();
        let sources = dir.path().join("sources");
        std::fs::create_dir_all(&sources).unwrap();
        assert!(find_install_image(dir.path()).is_none());

        std::fs::write(sources.join("install.esd"), b"x").unwrap();
        let found = find_install_image(dir.path()).unwrap();
        assert!(found.ends_with("sources/install.esd"));
    }
}
