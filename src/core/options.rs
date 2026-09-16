use serde::{Deserialize, Serialize};

use super::drivers::DriverSource;
use super::error::{CoreError, Result};

/// Partition map written to the target drive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PartitionScheme {
    /// GUID Partition Table. Required for UEFI boot (the common Windows To Go path).
    Gpt,
    /// Legacy MBR. Only for very old BIOS-only machines.
    Mbr,
}

/// Target file system for the Windows volume.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileSystem {
    Ntfs,
}

/// User-selected options for writing the drive.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct WriteOptions {
    pub partition_scheme: PartitionScheme,
    pub file_system: FileSystem,
    /// Install UEFI boot files (BOOTX64.EFI / BCD) on the ESP.
    pub uefi: bool,
    /// Skip the "image fits in target" precondition.
    pub skip_size_check: bool,
    /// Re-read written data and compare checksums. Very slow, off by default.
    pub verify_after_write: bool,
    /// Create a local administrator account during OOBE (skips the Microsoft
    /// account requirement). A matching `unattend.xml` is written to the image.
    pub create_local_account: bool,
    /// Name of the local administrator to create.
    pub local_username: String,
    /// Password for the local administrator; empty means no password.
    pub local_password: String,
    /// Set `OOBE\BypassNRO=1` so OOBE offers the offline account path even when
    /// no local account is pre-created.
    pub bypass_network: bool,
    /// Optional Steam hardware drivers staged on the drive and installed on
    /// first logon.
    #[serde(default)]
    pub drivers: DriverSource,
}

impl Default for WriteOptions {
    fn default() -> Self {
        Self {
            partition_scheme: PartitionScheme::Gpt,
            file_system: FileSystem::Ntfs,
            uefi: true,
            skip_size_check: false,
            verify_after_write: false,
            create_local_account: true,
            local_username: "user".to_owned(),
            local_password: String::new(),
            bypass_network: true,
            drivers: DriverSource::None,
        }
    }
}

impl WriteOptions {
    /// Validate that the combination of options can be written.
    pub fn validate(&self) -> Result<()> {
        if self.uefi && self.partition_scheme != PartitionScheme::Gpt {
            return Err(CoreError::InvalidConfig(
                "UEFI boot requires a GPT partition scheme".to_owned(),
            ));
        }
        if self.create_local_account && self.local_username.trim().is_empty() {
            return Err(CoreError::InvalidConfig(
                "the local administrator account needs a user name".to_owned(),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_valid() {
        assert!(WriteOptions::default().validate().is_ok());
    }

    #[test]
    fn uefi_with_mbr_is_rejected() {
        let options = WriteOptions {
            partition_scheme: PartitionScheme::Mbr,
            uefi: true,
            ..WriteOptions::default()
        };
        assert!(matches!(
            options.validate(),
            Err(CoreError::InvalidConfig(_))
        ));
    }

    #[test]
    fn mbr_without_uefi_is_allowed() {
        let options = WriteOptions {
            partition_scheme: PartitionScheme::Mbr,
            uefi: false,
            ..WriteOptions::default()
        };
        assert!(options.validate().is_ok());
    }

    #[test]
    fn empty_local_username_is_rejected() {
        let options = WriteOptions {
            local_username: "  ".to_owned(),
            ..WriteOptions::default()
        };
        assert!(matches!(
            options.validate(),
            Err(CoreError::InvalidConfig(_))
        ));
    }

    #[test]
    fn old_config_without_new_fields_uses_defaults() {
        let json = r#"{
            "partition_scheme": "gpt",
            "file_system": "ntfs",
            "uefi": true,
            "skip_size_check": false,
            "verify_after_write": false
        }"#;
        let options: WriteOptions = serde_json::from_str(json).unwrap();
        assert_eq!(options, WriteOptions::default());
    }
}
