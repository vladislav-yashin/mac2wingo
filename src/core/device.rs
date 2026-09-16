use serde::{Deserialize, Serialize};

use super::error::Result;

/// Physical transport used by a drive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BusType {
    Usb,
    Thunderbolt,
    FireWire,
    DiskImage,
    Internal,
    Unknown,
}

impl BusType {
    /// Parse the `BusProtocol` value reported by `diskutil info -plist`.
    #[must_use]
    pub fn from_bus_protocol(protocol: &str) -> Self {
        match protocol {
            "USB" => Self::Usb,
            "Thunderbolt" => Self::Thunderbolt,
            "FireWire" => Self::FireWire,
            "Disk Image" => Self::DiskImage,
            "Apple Fabric" | "PCI-Express" | "SATA" | "ATA" => Self::Internal,
            _ => Self::Unknown,
        }
    }

    /// Whether a Windows To Go target may live on this bus.
    #[must_use]
    pub fn is_external(self) -> bool {
        matches!(self, Self::Usb | Self::Thunderbolt | Self::FireWire)
    }
}

/// A single volume (partition) on a drive.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Volume {
    pub device_id: String,
    pub name: Option<String>,
    pub mount_point: Option<String>,
    pub file_system: Option<String>,
    pub size_bytes: u64,
}

/// Default logical block size used when the platform does not report one.
#[must_use]
pub fn default_block_size() -> u64 {
    512
}

/// A whole disk that can potentially be used as a Windows To Go target.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiskDevice {
    /// BSD device identifier, e.g. `disk5`.
    pub id: String,
    /// Human readable media name, e.g. `SanDisk Ultra USB 3.0`.
    pub name: String,
    pub size_bytes: u64,
    /// Logical block (sector) size in bytes. File systems must use clusters at
    /// least this large, so it is needed to format the ESP correctly.
    #[serde(default = "default_block_size")]
    pub block_size: u64,
    pub bus: BusType,
    pub is_removable: bool,
    pub is_internal: bool,
    /// Partition map, e.g. `GUID_partition_scheme`.
    pub partition_scheme: Option<String>,
    pub volumes: Vec<Volume>,
}

impl DiskDevice {
    /// A drive is a valid Windows To Go target when it is external, non-internal
    /// and reports a usable capacity.
    #[must_use]
    pub fn is_eligible_target(&self) -> bool {
        !self.is_internal && self.bus.is_external() && self.size_bytes > 0
    }

    #[must_use]
    pub fn size_human(&self) -> String {
        super::format::human_size(self.size_bytes)
    }
}

/// Read-only view over the system's storage devices.
pub trait DeviceService: Send + Sync {
    /// Enumerate physical whole disks known to the operating system.
    fn list_devices(&self) -> Result<Vec<DiskDevice>>;

    /// Refresh a single drive by its BSD identifier.
    fn device(&self, id: &str) -> Result<DiskDevice>;

    /// Convenience helper: only the drives eligible as a WTG target, newest APIs may
    /// grow additional filters here.
    fn list_targets(&self) -> Result<Vec<DiskDevice>> {
        let mut devices: Vec<_> = self
            .list_devices()?
            .into_iter()
            .filter(DiskDevice::is_eligible_target)
            .collect();
        devices.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(devices)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn usb_device() -> DiskDevice {
        DiskDevice {
            id: "disk5".into(),
            name: "SanDisk".into(),
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
    fn usb_is_external() {
        assert!(BusType::Usb.is_external());
        assert!(BusType::Thunderbolt.is_external());
        assert!(!BusType::Internal.is_external());
        assert!(!BusType::DiskImage.is_external());
    }

    #[test]
    fn eligible_target_requires_external_non_internal() {
        let mut device = usb_device();
        assert!(device.is_eligible_target());

        device.is_internal = true;
        assert!(!device.is_eligible_target());

        device = usb_device();
        device.bus = BusType::DiskImage;
        assert!(!device.is_eligible_target());

        device = usb_device();
        device.size_bytes = 0;
        assert!(!device.is_eligible_target());
    }

    #[test]
    fn bus_protocol_mapping() {
        assert_eq!(BusType::from_bus_protocol("USB"), BusType::Usb);
        assert_eq!(
            BusType::from_bus_protocol("Thunderbolt"),
            BusType::Thunderbolt
        );
        assert_eq!(BusType::from_bus_protocol("Disk Image"), BusType::DiskImage);
        assert_eq!(
            BusType::from_bus_protocol("Apple Fabric"),
            BusType::Internal
        );
        assert_eq!(BusType::from_bus_protocol("Nope"), BusType::Unknown);
    }
}
