//! Integration tests for the macOS `diskutil` device service using fixtures.

use mac2wingo::core::command::{Command, CommandRunner};
use mac2wingo::core::device::DeviceService;
use mac2wingo::core::error::CoreError;
use mac2wingo::platform::macos::diskutil::DiskutilDeviceService;

const LIST: &str = include_str!("fixtures/diskutil_list_mixed.plist");
const INFO_INTERNAL: &str = include_str!("fixtures/diskutil_info_internal.plist");
const INFO_USB: &str = include_str!("fixtures/diskutil_info_usb.plist");

struct FakeRunner;

impl CommandRunner for FakeRunner {
    fn run(&self, command: &Command) -> mac2wingo::core::error::Result<String> {
        assert_eq!(command.program, "/usr/sbin/diskutil");
        let args: Vec<&str> = command.args.iter().map(String::as_str).collect();
        match args.as_slice() {
            ["list", "-plist"] => Ok(LIST.to_owned()),
            ["info", "-plist", "disk0"] => Ok(INFO_INTERNAL.to_owned()),
            ["info", "-plist", "disk5"] => Ok(INFO_USB.to_owned()),
            other => panic!("unexpected diskutil invocation: {other:?}"),
        }
    }
}

fn service() -> DiskutilDeviceService {
    DiskutilDeviceService::with_runner(Box::new(FakeRunner))
}

#[test]
fn lists_all_whole_disks() {
    let devices = service().list_devices().unwrap();
    assert_eq!(devices.len(), 2);

    let internal = devices.iter().find(|d| d.id == "disk0").unwrap();
    assert!(internal.is_internal);
    assert!(!internal.is_eligible_target());

    let usb = devices.iter().find(|d| d.id == "disk5").unwrap();
    assert!(!usb.is_internal);
    assert!(usb.is_removable);
    assert!(usb.is_eligible_target());
    assert_eq!(usb.volumes.len(), 1);
    assert_eq!(usb.volumes[0].name.as_deref(), Some("WTG"));
}

#[test]
fn list_targets_filters_out_internal_drives() {
    let targets = service().list_targets().unwrap();
    assert_eq!(targets.len(), 1);
    assert_eq!(targets[0].id, "disk5");
}

#[test]
fn device_lookup_refreshes_single_drive() {
    let device = service().device("disk5").unwrap();
    assert_eq!(device.id, "disk5");
    assert_eq!(device.name, "SanDisk Ultra USB 3.0");
}

#[test]
fn rejects_invalid_device_ids() {
    let service = service();
    for id in ["", "disk", "sda", "disk5; rm -rf /"] {
        assert!(
            matches!(service.device(id), Err(CoreError::DeviceNotFound(_))),
            "expected `{id}` to be rejected"
        );
    }
}
