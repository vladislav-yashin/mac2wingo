//! Test doubles shared by unit and integration tests.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::core::device::{DeviceService, DiskDevice};
use crate::core::error::{CoreError, Result};
use crate::core::image::{ImageService, WindowsImage};
use crate::core::progress::{ProgressEvent, ProgressSink, Stage};
use crate::core::writer::{WindowsToGoWriter, WritePlan};

/// [`DeviceService`] returning a fixed set of drives.
#[derive(Debug, Default, Clone)]
pub struct MockDeviceService {
    devices: Vec<DiskDevice>,
}

impl MockDeviceService {
    #[must_use]
    pub fn new(devices: Vec<DiskDevice>) -> Self {
        Self { devices }
    }
}

impl DeviceService for MockDeviceService {
    fn list_devices(&self) -> Result<Vec<DiskDevice>> {
        Ok(self.devices.clone())
    }

    fn device(&self, id: &str) -> Result<DiskDevice> {
        self.devices
            .iter()
            .find(|device| device.id == id)
            .cloned()
            .ok_or_else(|| CoreError::DeviceNotFound(id.to_owned()))
    }
}

/// [`ImageService`] returning a preconfigured image.
#[derive(Debug, Clone)]
pub struct MockImageService {
    image: WindowsImage,
    fail: bool,
}

impl MockImageService {
    #[must_use]
    pub fn new(image: WindowsImage) -> Self {
        Self { image, fail: false }
    }

    /// An [`ImageService`] whose [`ImageService::inspect`] always fails.
    #[must_use]
    pub fn failing() -> Self {
        Self {
            image: WindowsImage {
                path: PathBuf::new(),
                format: crate::core::image::ImageFormat::Unknown,
                size_bytes: 0,
                edition: None,
                architecture: None,
                edition_index: 0,
                version: None,
                editions: Vec::new(),
            },
            fail: true,
        }
    }
}

impl ImageService for MockImageService {
    fn inspect(&self, path: &Path) -> Result<WindowsImage> {
        if self.fail {
            return Err(CoreError::UnsupportedImage(path.display().to_string()));
        }
        Ok(self.image.clone())
    }
}

/// A [`WindowsToGoWriter`] that records plans and can simulate failure.
#[derive(Debug, Default)]
pub struct MockWriter {
    inner: Mutex<MockWriterState>,
}

#[derive(Debug, Default)]
struct MockWriterState {
    plans: Vec<WritePlan>,
    fail: bool,
}

impl MockWriter {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// A writer that fails during [`Stage::ApplyingImage`].
    #[must_use]
    pub fn failing() -> Self {
        let writer = Self::default();
        writer.inner.lock().unwrap().fail = true;
        writer
    }

    /// Plans passed to [`WindowsToGoWriter::write`], in order.
    #[must_use]
    pub fn written_plans(&self) -> Vec<WritePlan> {
        self.inner.lock().unwrap().plans.clone()
    }
}

impl WindowsToGoWriter for MockWriter {
    fn write(&self, plan: &WritePlan, sink: &dyn ProgressSink) -> Result<()> {
        plan.validate()?;

        let mut state = self.inner.lock().unwrap();
        state.plans.push(plan.clone());
        let fail = state.fail;
        drop(state);

        sink.report(ProgressEvent::new(Stage::Preparing, 0.05));
        sink.report(ProgressEvent::new(Stage::ApplyingImage, 0.5));

        if fail {
            return Err(CoreError::Operation("simulated write failure".to_owned()));
        }

        for stage in Stage::ALL {
            sink.report(ProgressEvent::new(stage, 1.0));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::device::BusType;

    fn device(id: &str) -> DiskDevice {
        DiskDevice {
            id: id.into(),
            name: "Mock".into(),
            size_bytes: 32_000_000_000,
            block_size: 512,
            bus: BusType::Usb,
            is_removable: true,
            is_internal: false,
            partition_scheme: None,
            volumes: vec![],
        }
    }

    #[test]
    fn device_service_lists_and_finds() {
        let service = MockDeviceService::new(vec![device("disk5"), device("disk6")]);
        assert_eq!(service.list_devices().unwrap().len(), 2);
        assert_eq!(service.device("disk6").unwrap().id, "disk6");
        assert!(matches!(
            service.device("disk9"),
            Err(CoreError::DeviceNotFound(_))
        ));
    }
}
