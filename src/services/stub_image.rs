use std::path::Path;

use crate::core::error::{CoreError, Result};
use crate::core::image::{validate_extension, ImageService, WindowsImage};

/// Placeholder [`ImageService`].
///
/// It validates the file exists and has a supported extension, then reports the
/// on-disk size. Reading the real WIM metadata (`wimlib-imagex info`) is left
/// for the next stage.
#[derive(Debug, Default, Clone, Copy)]
pub struct StubImageService;

impl ImageService for StubImageService {
    fn inspect(&self, path: &Path) -> Result<WindowsImage> {
        if !path.exists() {
            return Err(CoreError::ImageNotFound(path.display().to_string()));
        }

        let format = validate_extension(path)?;
        let size_bytes = std::fs::metadata(path)?.len();

        Ok(WindowsImage {
            path: path.to_path_buf(),
            format,
            size_bytes,
            edition: None,
            architecture: None,
            edition_index: 0,
            version: None,
            editions: Vec::new(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_size_of_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("install.wim");
        std::fs::write(&path, b"fake wim contents").unwrap();

        let image = StubImageService.inspect(&path).unwrap();
        assert_eq!(image.size_bytes, 17);
        assert_eq!(image.format, crate::core::image::ImageFormat::Wim);
        assert!(image.edition.is_none());
    }

    #[test]
    fn errors_when_missing() {
        let err = StubImageService
            .inspect(Path::new("/definitely/not/here.iso"))
            .unwrap_err();
        assert!(matches!(err, CoreError::ImageNotFound(_)));
    }

    #[test]
    fn errors_on_unsupported_extension() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("archive.zip");
        std::fs::write(&path, b"x").unwrap();
        assert!(matches!(
            StubImageService.inspect(&path),
            Err(CoreError::UnsupportedImage(_))
        ));
    }
}
