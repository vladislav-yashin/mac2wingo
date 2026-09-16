use crate::core::device::DiskDevice;
use crate::core::error::Result;
use crate::core::image::WindowsImage;
use crate::core::options::WriteOptions;
use crate::core::progress::ProgressSink;

/// Everything required to produce a Windows To Go drive.
#[derive(Debug, Clone, PartialEq)]
pub struct WritePlan {
    pub device: DiskDevice,
    pub image: WindowsImage,
    pub options: WriteOptions,
}

impl WritePlan {
    /// Validate the plan before any destructive work starts.
    pub fn validate(&self) -> Result<()> {
        if !self.device.is_eligible_target() {
            return Err(crate::core::error::CoreError::NotEligible(
                self.device.id.clone(),
            ));
        }
        self.options.validate()?;

        // Steam hardware drivers are 64-bit x86 only (Steam Deck / Steam
        // Machine). Block them when the image architecture is known to differ.
        if self.options.drivers.is_enabled() {
            if let Some(architecture) = self.image.architecture.as_deref() {
                if architecture != "x86_64" {
                    return Err(crate::core::error::CoreError::InvalidConfig(format!(
                        "Steam hardware drivers require an x86_64 image, but this one is {architecture}"
                    )));
                }
            }
        }

        if !self.options.skip_size_check {
            // A full Windows install needs roughly 20 GiB; warn-in-UI logic lives
            // in the app layer, this is the hard floor for the stub.
            const MIN_TARGET_BYTES: u64 = 16 * 1024 * 1024 * 1024;
            if self.device.size_bytes < MIN_TARGET_BYTES {
                return Err(crate::core::error::CoreError::InvalidConfig(
                    "target drive is too small for Windows To Go".to_owned(),
                ));
            }
        }
        Ok(())
    }
}

/// Writes a Windows To Go image to a target drive.
///
/// This is the seam where the real, privileged implementation will be plugged
/// in. Stage 1 ships [`crate::services::stub_writer::StubWriter`].
pub trait WindowsToGoWriter: Send + Sync {
    /// Perform the write, reporting progress through `sink`.
    ///
    /// Implementations must never touch the target when [`WritePlan::validate`]
    /// returns an error.
    fn write(&self, plan: &WritePlan, sink: &dyn ProgressSink) -> Result<()>;

    /// Request cancellation of an in-flight write. Best effort.
    fn cancel(&self) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::device::BusType;

    fn device(size_bytes: u64) -> DiskDevice {
        DiskDevice {
            id: "disk5".into(),
            name: "SanDisk".into(),
            size_bytes,
            block_size: 512,
            bus: BusType::Usb,
            is_removable: true,
            is_internal: false,
            partition_scheme: None,
            volumes: vec![],
        }
    }

    fn image() -> WindowsImage {
        WindowsImage {
            path: "/tmp/win11.iso".into(),
            format: crate::core::image::ImageFormat::Iso,
            size_bytes: 6_000_000_000,
            edition: None,
            architecture: None,
            edition_index: 1,
            version: None,
            editions: Vec::new(),
        }
    }

    fn plan(size_bytes: u64) -> WritePlan {
        WritePlan {
            device: device(size_bytes),
            image: image(),
            options: WriteOptions::default(),
        }
    }

    #[test]
    fn valid_plan_is_accepted() {
        assert!(plan(32_000_000_000).validate().is_ok());
    }

    #[test]
    fn tiny_target_is_rejected() {
        assert!(plan(8_000_000_000).validate().is_err());
    }

    #[test]
    fn skip_size_check_allows_tiny_target() {
        let mut plan = plan(8_000_000_000);
        plan.options.skip_size_check = true;
        assert!(plan.validate().is_ok());
    }

    #[test]
    fn steam_drivers_require_x86_64() {
        use crate::core::drivers::{DriverSource, SteamHardware};

        let mut plan = plan(32_000_000_000);
        plan.options.drivers = DriverSource::Auto(SteamHardware::DeckOled);

        plan.image.architecture = Some("x86_64".into());
        assert!(plan.validate().is_ok());

        plan.image.architecture = Some("ARM64".into());
        assert!(matches!(
            plan.validate(),
            Err(crate::core::error::CoreError::InvalidConfig(_))
        ));
    }

    #[test]
    fn unknown_architecture_still_allows_drivers() {
        use crate::core::drivers::{DriverSource, SteamHardware};

        let mut plan = plan(32_000_000_000);
        plan.options.drivers = DriverSource::Auto(SteamHardware::DeckLcd);
        plan.image.architecture = None;
        assert!(plan.validate().is_ok());
    }
}
