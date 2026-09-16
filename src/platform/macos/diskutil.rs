//! Parsing of `diskutil` property lists and the [`DeviceService`] built on top.
//!
//! Parsing is split from process execution via [`CommandRunner`] so that it can
//! be unit-tested against recorded fixtures without touching real hardware.

use serde::Deserialize;

use crate::core::command::{Command, CommandRunner, SystemCommandRunner};
use crate::core::device::{default_block_size, BusType, DiskDevice, Volume};
use crate::core::error::{CoreError, Result};

const DISKUTIL: &str = "/usr/sbin/diskutil";

#[derive(Debug, Deserialize)]
struct ListOutput {
    #[serde(rename = "AllDisksAndPartitions", default)]
    all_disks_and_partitions: Vec<ListDisk>,
    #[serde(rename = "WholeDisks", default)]
    whole_disks: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct ListDisk {
    #[serde(rename = "DeviceIdentifier")]
    device_identifier: String,
    #[serde(rename = "Size", default)]
    size: u64,
    #[serde(rename = "Content")]
    content: Option<String>,
    #[serde(rename = "Partitions", default)]
    partitions: Vec<ListPartition>,
}

#[derive(Debug, Deserialize)]
struct ListPartition {
    #[serde(rename = "DeviceIdentifier")]
    device_identifier: String,
    #[serde(rename = "Size", default)]
    size: u64,
    #[serde(rename = "Content")]
    content: Option<String>,
    #[serde(rename = "VolumeName")]
    volume_name: Option<String>,
    #[serde(rename = "MountPoint")]
    mount_point: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct InfoOutput {
    device_identifier: String,
    media_name: Option<String>,
    size: Option<u64>,
    total_size: Option<u64>,
    #[serde(default = "default_block_size")]
    device_block_size: u64,
    #[serde(default)]
    internal: bool,
    #[serde(default)]
    removable_media: bool,
    bus_protocol: Option<String>,
    content: Option<String>,
}

/// One whole disk as reported by `diskutil list -plist`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListedDisk {
    pub id: String,
    pub size_bytes: u64,
    pub content: Option<String>,
    pub volumes: Vec<Volume>,
}

/// Parse the `AllDisksAndPartitions` structure, keeping only whole disks.
pub fn parse_list(xml: &str) -> Result<Vec<ListedDisk>> {
    let output: ListOutput =
        plist::from_bytes(xml.as_bytes()).map_err(|error| CoreError::Parse(error.to_string()))?;

    let whole: std::collections::HashSet<&str> =
        output.whole_disks.iter().map(String::as_str).collect();

    let disks = output
        .all_disks_and_partitions
        .into_iter()
        .filter(|disk| whole.is_empty() || whole.contains(disk.device_identifier.as_str()))
        .map(|disk| {
            let volumes = disk
                .partitions
                .into_iter()
                .map(|partition| Volume {
                    device_id: partition.device_identifier,
                    name: partition.volume_name,
                    mount_point: partition.mount_point,
                    file_system: partition.content,
                    size_bytes: partition.size,
                })
                .collect();
            ListedDisk {
                id: disk.device_identifier,
                size_bytes: disk.size,
                content: disk.content,
                volumes,
            }
        })
        .collect();

    Ok(disks)
}

/// Raw fields extracted from `diskutil info -plist`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiskInfo {
    pub id: String,
    pub media_name: Option<String>,
    pub size_bytes: u64,
    pub block_size: u64,
    pub is_internal: bool,
    pub is_removable: bool,
    pub bus: BusType,
    pub content: Option<String>,
}

/// Parse a `diskutil info -plist` document.
pub fn parse_info(xml: &str) -> Result<DiskInfo> {
    let info: InfoOutput =
        plist::from_bytes(xml.as_bytes()).map_err(|error| CoreError::Parse(error.to_string()))?;

    let bus = info
        .bus_protocol
        .as_deref()
        .map(BusType::from_bus_protocol)
        .unwrap_or(BusType::Unknown);

    Ok(DiskInfo {
        id: info.device_identifier,
        media_name: info.media_name,
        size_bytes: info.size.or(info.total_size).unwrap_or_default(),
        block_size: info.device_block_size,
        is_internal: info.internal || bus == BusType::Internal,
        is_removable: info.removable_media,
        bus,
        content: info.content,
    })
}

/// Merge the summary from `list` with the detailed `info` document.
#[must_use]
pub fn combine(listed: ListedDisk, info: DiskInfo) -> DiskDevice {
    DiskDevice {
        id: info.id,
        name: info
            .media_name
            .clone()
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| listed.id.clone()),
        size_bytes: if info.size_bytes > 0 {
            info.size_bytes
        } else {
            listed.size_bytes
        },
        block_size: info.block_size,
        bus: info.bus,
        is_removable: info.is_removable,
        is_internal: info.is_internal,
        partition_scheme: info.content.or(listed.content),
        volumes: listed.volumes,
    }
}

/// [`DeviceService`](crate::core::device::DeviceService) backed by `diskutil`.
pub struct DiskutilDeviceService {
    runner: Box<dyn CommandRunner>,
}

impl DiskutilDeviceService {
    #[must_use]
    pub fn system() -> Self {
        Self {
            runner: Box::new(SystemCommandRunner),
        }
    }

    #[must_use]
    pub fn with_runner(runner: Box<dyn CommandRunner>) -> Self {
        Self { runner }
    }
}

impl Default for DiskutilDeviceService {
    fn default() -> Self {
        Self::system()
    }
}

impl crate::core::device::DeviceService for DiskutilDeviceService {
    fn list_devices(&self) -> Result<Vec<DiskDevice>> {
        let output = self
            .runner
            .run(&Command::new(DISKUTIL).args(["list", "-plist"]))?;
        let listed = parse_list(&output)?;

        let mut devices = Vec::with_capacity(listed.len());
        for disk in listed {
            let info_xml = self
                .runner
                .run(&Command::new(DISKUTIL).args(["info", "-plist", &disk.id]))?;
            let info = parse_info(&info_xml)?;
            devices.push(combine(disk, info));
        }
        Ok(devices)
    }

    fn device(&self, id: &str) -> Result<DiskDevice> {
        if !is_valid_device_id(id) {
            return Err(CoreError::DeviceNotFound(id.to_owned()));
        }
        let info_xml = self
            .runner
            .run(&Command::new(DISKUTIL).args(["info", "-plist", id]))?;
        let info = parse_info(&info_xml)?;
        let listed = ListedDisk {
            id: info.id.clone(),
            size_bytes: info.size_bytes,
            content: info.content.clone(),
            volumes: Vec::new(),
        };
        Ok(combine(listed, info))
    }
}

/// Guard against passing arbitrary strings to the `diskutil` CLI.
#[must_use]
pub fn is_valid_device_id(id: &str) -> bool {
    id.starts_with("disk")
        && !id[4..].is_empty()
        && id[4..].chars().all(|ch| ch.is_ascii_alphanumeric())
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIST: &str = include_str!("../../../tests/fixtures/diskutil_list.plist");
    const INFO_INTERNAL: &str =
        include_str!("../../../tests/fixtures/diskutil_info_internal.plist");
    const INFO_DISKIMAGE: &str =
        include_str!("../../../tests/fixtures/diskutil_info_diskimage.plist");
    const INFO_USB: &str = include_str!("../../../tests/fixtures/diskutil_info_usb.plist");

    #[test]
    fn parses_whole_disks() {
        let disks = parse_list(LIST).unwrap();
        // The fixture reports 5 whole disks.
        assert_eq!(disks.len(), 5);
        assert!(disks.iter().any(|disk| disk.id == "disk0"));
        assert!(disks.iter().any(|disk| disk.id == "disk4"));

        let disk4 = disks.iter().find(|disk| disk.id == "disk4").unwrap();
        assert_eq!(disk4.volumes.len(), 1);
        assert_eq!(disk4.volumes[0].name.as_deref(), Some("Happ v4.2.1"));
    }

    #[test]
    fn parses_internal_info() {
        let info = parse_info(INFO_INTERNAL).unwrap();
        assert_eq!(info.id, "disk0");
        assert!(info.is_internal);
        assert!(!info.is_removable);
        assert_eq!(info.bus, BusType::Internal);
        assert_eq!(info.block_size, 4096);
        assert_eq!(info.media_name.as_deref(), Some("APPLE SSD AP0256Q"));
    }

    #[test]
    fn parses_disk_image_info() {
        let info = parse_info(INFO_DISKIMAGE).unwrap();
        assert_eq!(info.id, "disk4");
        assert!(!info.is_internal);
        assert!(info.is_removable);
        assert_eq!(info.bus, BusType::DiskImage);
    }

    #[test]
    fn parses_usb_info() {
        let info = parse_info(INFO_USB).unwrap();
        assert_eq!(info.id, "disk5");
        assert!(!info.is_internal);
        assert!(info.is_removable);
        assert_eq!(info.bus, BusType::Usb);
        assert_eq!(info.block_size, 512);
        assert_eq!(info.media_name.as_deref(), Some("SanDisk Ultra USB 3.0"));
    }

    #[test]
    fn combines_list_and_info_into_eligible_device() {
        let listed = parse_list(LIST)
            .unwrap()
            .into_iter()
            .find(|disk| disk.id == "disk4")
            .unwrap();
        let info = parse_info(INFO_DISKIMAGE).unwrap();
        let device = combine(listed, info);
        assert_eq!(device.id, "disk4");
        assert_eq!(device.name, "Disk Image");
        assert!(!device.is_eligible_target());
    }

    #[test]
    fn rejects_malformed_plist() {
        assert!(matches!(
            parse_list("not a plist"),
            Err(CoreError::Parse(_))
        ));
    }

    #[test]
    fn device_id_validation() {
        assert!(is_valid_device_id("disk5"));
        assert!(is_valid_device_id("disk10s2"));
        assert!(!is_valid_device_id(""));
        assert!(!is_valid_device_id("disk"));
        assert!(!is_valid_device_id("disk5; rm -rf /"));
        assert!(!is_valid_device_id("sda"));
    }
}
