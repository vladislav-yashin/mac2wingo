//! Optional Steam hardware driver integration.
//!
//! Valve publishes the Windows drivers for its hardware on an official CDN
//! (`steamdeck-packages.steamos.cloud`). Mac2Win Go can either download those
//! packages automatically or use ZIPs/folders the user downloaded beforehand.
//!
//! The drivers themselves cannot be slip-streamed offline the way DISM does it
//! (we apply `install.wim` directly, so the `offlineServicing` pass never runs).
//! Instead they are staged on the target partition and installed automatically
//! with `pnputil` on first logon; see [`crate::services::real_writer`].

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Valve hardware models that have Windows driver packages available.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SteamHardware {
    /// Original Steam Deck (LCD).
    DeckLcd,
    /// Steam Deck OLED.
    DeckOled,
    /// Steam Machine (2026).
    Machine,
}

impl SteamHardware {
    /// Every supported model, in display order.
    pub const ALL: [SteamHardware; 3] = [
        SteamHardware::DeckOled,
        SteamHardware::DeckLcd,
        SteamHardware::Machine,
    ];

    /// Stable i18n key for the model name.
    #[must_use]
    pub fn key(self) -> &'static str {
        match self {
            Self::DeckLcd => "options.drivers.lcd",
            Self::DeckOled => "options.drivers.oled",
            Self::Machine => "options.drivers.machine",
        }
    }
}

/// A single official driver archive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DriverPackage {
    /// Human readable component name.
    pub name: &'static str,
    /// Directory the archive is extracted into.
    pub folder: &'static str,
    /// File name used when downloading.
    pub file_name: &'static str,
    /// Official download URL.
    pub url: &'static str,
}

/// Official packages for the Steam Deck LCD.
const DECK_LCD: &[DriverPackage] = &[
    DriverPackage {
        name: "APU",
        folder: "APU",
        file_name: "Aerith_Sephiroth_Windows_Driver_2309131113.zip",
        url: "https://steamdeck-packages.steamos.cloud/misc/windows/drivers/Aerith_Sephiroth_Windows_Driver_2309131113.zip",
    },
    DriverPackage {
        name: "Wi-Fi",
        folder: "WiFi",
        file_name: "RTLWlanE_WindowsDriver_2024.0.10.137_Drv_3.00.0039_Win11.L.zip",
        url: "https://steamdeck-packages.steamos.cloud/misc/windows/drivers/RTLWlanE_WindowsDriver_2024.0.10.137_Drv_3.00.0039_Win11.L.zip",
    },
    DriverPackage {
        name: "Bluetooth",
        folder: "Bluetooth",
        file_name: "RTBlueR_FilterDriver_1041.3005_1201.2021_new_L.zip",
        url: "https://steamdeck-packages.steamos.cloud/misc/windows/drivers/RTBlueR_FilterDriver_1041.3005_1201.2021_new_L.zip",
    },
    DriverPackage {
        name: "SD card reader",
        folder: "SDCard",
        file_name: "BayHub_SD_STOR_installV3.4.01.89_W10W11_logoed_20220228.zip",
        url: "https://steamdeck-packages.steamos.cloud/misc/windows/drivers/BayHub_SD_STOR_installV3.4.01.89_W10W11_logoed_20220228.zip",
    },
    DriverPackage {
        name: "Audio (cs35l41)",
        folder: "Audio1",
        file_name: "cs35l41-V1.2.1.0.zip",
        url: "https://steamdeck-packages.steamos.cloud/misc/windows/drivers/cs35l41-V1.2.1.0.zip",
    },
    DriverPackage {
        name: "Audio (NAU88L21)",
        folder: "Audio2",
        file_name: "NAU88L21_x64_1.0.6.0_WHQL.zip",
        url: "https://steamdeck-packages.steamos.cloud/misc/windows/drivers/NAU88L21_x64_1.0.6.0_WHQL%20-%20DUA_BIQ_WHQL.zip",
    },
];

/// Official packages for the Steam Deck OLED.
const DECK_OLED: &[DriverPackage] = &[
    DriverPackage {
        name: "APU",
        folder: "APU",
        file_name: "GFX_Driver_48.0.8.40630.zip",
        url: "https://steamdeck-packages.steamos.cloud/misc/windows/drivers/GFX_Driver_48.0.8.40630.zip",
    },
    DriverPackage {
        name: "Wi-Fi",
        folder: "WiFi",
        file_name: "FC66E-WIN_WiFi_driver.zip",
        url: "https://steamdeck-packages.steamos.cloud/misc/windows/drivers/FC66E-WIN_WiFi_driver.zip",
    },
    DriverPackage {
        name: "Bluetooth",
        folder: "Bluetooth",
        file_name: "FC66E-B_WIN_Bluetooth_driver.zip",
        url: "https://steamdeck-packages.steamos.cloud/misc/windows/drivers/FC66E-B_WIN_Bluetooth_driver.zip",
    },
    DriverPackage {
        name: "SD card reader",
        folder: "SDCard",
        file_name: "BayHub_SD_STOR_installV3.4.01.89_W10W11_logoed_20220228.zip",
        url: "https://steamdeck-packages.steamos.cloud/misc/windows/drivers/BayHub_SD_STOR_installV3.4.01.89_W10W11_logoed_20220228.zip",
    },
    DriverPackage {
        name: "Audio (cs35l41)",
        folder: "Audio1",
        file_name: "Audio1_cs35l41-V1.2.1.0.zip",
        url: "https://steamdeck-packages.steamos.cloud/misc/windows/drivers/Audio1_cs35l41-V1.2.1.0.zip",
    },
    DriverPackage {
        name: "Audio (NAU88L21)",
        folder: "Audio2",
        file_name: "Audio2_NAU88L21_x64_1.0.9.1_WHQL.zip",
        url: "https://steamdeck-packages.steamos.cloud/misc/windows/drivers/Audio2_NAU88L21_x64_1.0.9.1_WHQL.zip",
    },
    DriverPackage {
        name: "Audio (AMD ACP)",
        folder: "Audio3",
        file_name: "amdampdriver.zip",
        url: "https://steamdeck-packages.steamos.cloud/misc/windows/drivers/amdampdriver.zip",
    },
];

/// Official packages for the Steam Machine.
const MACHINE: &[DriverPackage] = &[
    DriverPackage {
        name: "Graphics",
        folder: "GFX",
        file_name: "GFX_Driver_241126a-410993C.zip",
        url: "https://steamdeck-packages.steamos.cloud/misc/windows/drivers/machine/GFX/GFX%20Driver%20241126a-410993C.zip",
    },
    DriverPackage {
        name: "Wi-Fi",
        folder: "WiFi",
        file_name: "2.0.125.1229_QCA206x.zip",
        url: "https://steamdeck-packages.steamos.cloud/misc/windows/drivers/machine/W-Fi/2.0.125.1229_QCA206x.zip",
    },
    DriverPackage {
        name: "Bluetooth",
        folder: "Bluetooth",
        file_name: "FC66E-B_ACMD_WIN_BT_driver.zip",
        url: "https://steamdeck-packages.steamos.cloud/misc/windows/drivers/machine/BT/FC66E-B_ACMD_WIN_BT_driver.zip",
    },
    DriverPackage {
        name: "SD card reader",
        folder: "SDCard",
        file_name: "SDReader-20260702.zip",
        url: "https://steamdeck-packages.steamos.cloud/misc/windows/drivers/machine/SDReader/SDReader-20260702.zip",
    },
];

/// Official driver packages for a model.
#[must_use]
pub fn packages(hardware: SteamHardware) -> &'static [DriverPackage] {
    match hardware {
        SteamHardware::DeckLcd => DECK_LCD,
        SteamHardware::DeckOled => DECK_OLED,
        SteamHardware::Machine => MACHINE,
    }
}

/// Where the Steam hardware drivers come from.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DriverSource {
    /// Do not integrate drivers.
    #[default]
    None,
    /// Download the official packages for the selected model.
    Auto(SteamHardware),
    /// Use ZIP archives and/or extracted folders the user selected.
    Local(Vec<PathBuf>),
}

impl DriverSource {
    /// Whether any driver integration was requested.
    #[must_use]
    pub fn is_enabled(&self) -> bool {
        !matches!(self, Self::None)
    }

    /// Human readable list of the locally selected paths, sorted for stability.
    #[must_use]
    pub fn local_summary(&self) -> String {
        let Self::Local(paths) = self else {
            return String::new();
        };
        let mut names: Vec<String> = paths
            .iter()
            .map(|path| {
                path.file_name().map_or_else(
                    || path.display().to_string(),
                    |name| name.to_string_lossy().into_owned(),
                )
            })
            .collect();
        names.sort();
        names.join(", ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_model_has_at_least_one_package() {
        for hardware in SteamHardware::ALL {
            assert!(
                !packages(hardware).is_empty(),
                "{hardware:?} has no packages"
            );
        }
    }

    #[test]
    fn urls_are_https_zip_archives() {
        for hardware in SteamHardware::ALL {
            for package in packages(hardware) {
                assert!(package.url.starts_with("https://"), "{}", package.url);
                assert!(package.url.ends_with(".zip"), "{}", package.url);
                assert!(package.file_name.ends_with(".zip"), "{}", package.file_name);
                assert!(!package.folder.is_empty(), "{} has no folder", package.name);
            }
        }
    }

    #[test]
    fn file_names_are_unique_within_a_model() {
        for hardware in SteamHardware::ALL {
            let mut names: Vec<&str> = packages(hardware)
                .iter()
                .map(|package| package.file_name)
                .collect();
            names.sort_unstable();
            let count = names.len();
            names.dedup();
            assert_eq!(count, names.len(), "duplicate file name for {hardware:?}");
        }
    }

    #[test]
    fn default_source_is_disabled() {
        assert!(!DriverSource::default().is_enabled());
        assert!(DriverSource::Auto(SteamHardware::DeckOled).is_enabled());
        assert!(DriverSource::Local(vec![PathBuf::from("/tmp/x.zip")]).is_enabled());
    }

    #[test]
    fn serde_round_trips_variants() {
        let values = [
            DriverSource::None,
            DriverSource::Auto(SteamHardware::DeckLcd),
            DriverSource::Local(vec![PathBuf::from("/tmp/drivers")]),
        ];
        for value in values {
            let json = serde_json::to_string(&value).unwrap();
            let decoded: DriverSource = serde_json::from_str(&json).unwrap();
            assert_eq!(decoded, value);
        }
    }
}
