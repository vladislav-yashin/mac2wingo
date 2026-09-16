//! Real [`WindowsToGoWriter`] that partitions, formats and populates a drive.
//!
//! All destructive commands are collected into a single shell script which is
//! executed once with administrator privileges. This keeps the macOS
//! authorization prompt to a single occurrence while still streaming output for
//! progress. Stage markers (`::m2w:<token>`) embedded in the script let the UI
//! follow along.
//!
//! The pipeline:
//! 1. unmount the target and resolve the install image;
//! 2. `diskutil partitionDisk` (GPT + EFI, or legacy MBR);
//! 3. `newfs_msdos` for the ESP and `mkntfs` for the Windows volume;
//! 4. `wimlib-imagex apply` **directly to the raw NTFS partition** (libntfs-3g,
//!    no macFUSE required), preserving ACLs and attributes;
//! 5. patch the applied install offline (`ntfscat`/`hiveset`/`ntfscp`):
//!    `PortableOperatingSystem`, `SanPolicy`, hibernation and an `unattend.xml`;
//! 6. BCD-SYS copies the boot files and writes the BCD store;
//! 7. optionally verify, then eject.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::core::command::{shell_quote, Command, CommandRunner};
use crate::core::drivers::{self, DriverSource};
use crate::core::error::{CoreError, Result};
use crate::core::format::human_size;
use crate::core::image::ImageFormat;
use crate::core::options::{PartitionScheme, WriteOptions};
use crate::core::progress::{ProgressEvent, ProgressSink, Stage};
use crate::core::writer::{WindowsToGoWriter, WritePlan};
use crate::platform::macos::diskutil::is_valid_device_id;
use crate::platform::macos::privileged::PrivilegedCommandRunner;
use crate::platform::macos::tools::{Tool, ToolLocator};
use crate::services::real_image::IsoMount;

const DISKUTIL: &str = "/usr/sbin/diskutil";
const NEWFS_MSDOS: &str = "/sbin/newfs_msdos";
const SENTINEL: &str = "::m2w:";
const WINDOWS_LABEL: &str = "WINDOWS";
/// Paths inside the applied image that the offline configuration touches.
const SYSTEM_HIVE: &str = "Windows/System32/config/SYSTEM";
const SOFTWARE_HIVE: &str = "Windows/System32/config/SOFTWARE";
const UNATTEND_PANTHER: &str = "Windows/Panther/unattend.xml";
const UNATTEND_SYSPREP: &str = "Windows/System32/Sysprep/unattend.xml";
/// Folder on the Windows volume that holds the staged Steam hardware drivers.
const DRIVERS_FOLDER: &str = "SteamDeckDrivers";
/// Temporary WIM used to create the driver folder on the NTFS volume.
const DRIVERS_WIM: &str = "drivers.wim";
const CURL: &str = "/usr/bin/curl";
const DITTO: &str = "/usr/bin/ditto";

/// Real writer for Windows To Go drives.
pub struct RealWriter {
    runner: Arc<dyn CommandRunner>,
    tools: ToolLocator,
    cancelled: AtomicBool,
}

impl RealWriter {
    /// Production writer: privileged commands via `osascript`, bundled tools.
    #[must_use]
    pub fn system() -> Self {
        Self {
            runner: Arc::new(PrivilegedCommandRunner::default()),
            tools: ToolLocator::system(),
            cancelled: AtomicBool::new(false),
        }
    }

    /// Writer with injected dependencies (used in tests).
    #[must_use]
    pub fn with_runner(runner: Arc<dyn CommandRunner>, tools: ToolLocator) -> Self {
        Self {
            runner,
            tools,
            cancelled: AtomicBool::new(false),
        }
    }

    fn check_cancelled(&self) -> Result<()> {
        if self.cancelled.load(Ordering::SeqCst) {
            Err(CoreError::Cancelled)
        } else {
            Ok(())
        }
    }

    fn preflight(&self, plan: &WritePlan) -> Result<()> {
        let required: Vec<Tool> = match plan.options.partition_scheme {
            PartitionScheme::Gpt => Tool::UEFI.to_vec(),
            PartitionScheme::Mbr => Tool::ALL.to_vec(),
        };
        self.tools.preflight(&required)
    }

    /// Reject plans where the image clearly will not fit, unless the user
    /// explicitly opted out of the size check. `extra_bytes` accounts for the
    /// staged Steam hardware drivers, whose size is only known after download.
    fn ensure_capacity(&self, plan: &WritePlan, extra_bytes: u64) -> Result<()> {
        if plan.options.skip_size_check {
            return Ok(());
        }
        let image_bytes = plan
            .image
            .editions
            .iter()
            .find(|edition| edition.index == plan.image.edition_index)
            .and_then(|edition| edition.size_bytes)
            .unwrap_or(plan.image.size_bytes);
        // Leave room for the ESP, NTFS metadata and boot files.
        let required = image_bytes
            .saturating_add(2 * 1024 * 1024 * 1024)
            .saturating_add(extra_bytes);
        if required > plan.device.size_bytes {
            return Err(CoreError::InvalidConfig(format!(
                "the selected image needs about {} but `{}` only provides {}",
                human_size(required),
                plan.device.id,
                human_size(plan.device.size_bytes)
            )));
        }
        Ok(())
    }

    /// Resolve the WIM/ESD to apply, mounting the ISO when necessary. The mount
    /// guard must be kept alive until the write completes.
    fn resolve_install_image(&self, plan: &WritePlan) -> Result<(PathBuf, Option<IsoMount>)> {
        match plan.image.format {
            ImageFormat::Iso => {
                let mount = IsoMount::mount(Arc::clone(&self.runner), &plan.image.path)?;
                let path = mount.install_image()?;
                Ok((path, Some(mount)))
            }
            ImageFormat::Wim | ImageFormat::Esd => Ok((plan.image.path.clone(), None)),
            ImageFormat::Unknown => Err(CoreError::UnsupportedImage(
                plan.image.path.display().to_string(),
            )),
        }
    }

    /// Download and/or extract the Steam hardware drivers into the staging
    /// directory. Returns the number of driver bytes staged. This runs before
    /// any destructive work, so a failure leaves the target untouched.
    fn prepare_drivers(
        &self,
        plan: &WritePlan,
        mounts: &MountPoints,
        sink: &dyn ProgressSink,
    ) -> Result<u64> {
        if !plan.options.drivers.is_enabled() {
            return Ok(0);
        }

        sink.report(
            ProgressEvent::new(
                Stage::PreparingDrivers,
                Stage::PreparingDrivers.start_fraction(),
            )
            .with_detail("Staging Steam hardware drivers"),
        );

        std::fs::create_dir_all(&mounts.drivers_downloads).map_err(CoreError::Io)?;
        let package_root = mounts.drivers_root.join(DRIVERS_FOLDER);
        std::fs::create_dir_all(&package_root).map_err(CoreError::Io)?;

        match &plan.options.drivers {
            DriverSource::None => {}
            DriverSource::Auto(hardware) => {
                for package in drivers::packages(*hardware) {
                    self.check_cancelled()?;
                    let archive = mounts.drivers_downloads.join(package.file_name);
                    sink.report(
                        ProgressEvent::new(
                            Stage::PreparingDrivers,
                            Stage::PreparingDrivers.start_fraction(),
                        )
                        .with_detail(format!("Downloading {}", package.name)),
                    );
                    self.download(package.url, &archive)?;
                    self.check_cancelled()?;
                    self.extract_zip(&archive, &package_root.join(package.folder))?;
                }
            }
            DriverSource::Local(paths) => {
                if paths.is_empty() {
                    return Err(CoreError::InvalidConfig(
                        "no driver files or folders were selected".to_owned(),
                    ));
                }
                for (index, path) in paths.iter().enumerate() {
                    self.check_cancelled()?;
                    let destination = package_root.join(format!("Local{}", index + 1));
                    if path.is_dir() {
                        copy_dir_all(path, &destination)?;
                    } else if is_zip(path) {
                        self.extract_zip(path, &destination)?;
                    } else {
                        return Err(CoreError::InvalidConfig(format!(
                            "`{}` is neither a folder nor a ZIP archive",
                            path.display()
                        )));
                    }
                }
            }
        }

        self.extract_nested_archives(&package_root, sink)?;

        if find_first_inf(&package_root).is_none() {
            return Err(CoreError::InvalidConfig(
                "no `.inf` driver files were found in the selected source".to_owned(),
            ));
        }

        Ok(dir_size(&package_root))
    }

    /// Download a URL to a local file with `curl`.
    fn download(&self, url: &str, destination: &Path) -> Result<()> {
        self.runner
            .run(&Command::new(CURL).args([
                "--fail",
                "--location",
                "--retry",
                "3",
                "--output",
                destination.to_string_lossy().as_ref(),
                url,
            ]))
            .map(|_| ())
    }

    /// Extract a ZIP archive with `ditto` (always present on macOS).
    fn extract_zip(&self, archive: &Path, destination: &Path) -> Result<()> {
        std::fs::create_dir_all(destination).map_err(CoreError::Io)?;
        self.runner
            .run(&Command::new(DITTO).args([
                "-x",
                "-k",
                archive.to_string_lossy().as_ref(),
                destination.to_string_lossy().as_ref(),
            ]))
            .map(|_| ())
    }

    /// Extract ZIP archives nested inside the staged packages (some vendor
    /// packages ship as a ZIP inside a ZIP).
    fn extract_nested_archives(&self, root: &Path, sink: &dyn ProgressSink) -> Result<()> {
        for _ in 0..2 {
            let mut archives = Vec::new();
            collect_files_with_extension(root, "zip", &mut archives)?;
            if archives.is_empty() {
                break;
            }
            for archive in archives {
                self.check_cancelled()?;
                sink.report(
                    ProgressEvent::new(
                        Stage::PreparingDrivers,
                        Stage::PreparingDrivers.start_fraction(),
                    )
                    .with_detail(format!("Extracting {}", file_name(&archive))),
                );
                let destination = archive.with_extension("");
                self.extract_zip(&archive, &destination)?;
                let _ = std::fs::remove_file(&archive);
            }
        }
        Ok(())
    }

    /// Build the privileged shell script for the whole operation.
    fn build_script(
        &self,
        plan: &WritePlan,
        install_image: &Path,
        mounts: &MountPoints,
    ) -> Result<String> {
        let mut script = Script::new();
        let disk = format!("/dev/{}", plan.device.id);
        let raw_disk = format!("/dev/r{}", plan.device.id);
        let gpt = plan.options.partition_scheme == PartitionScheme::Gpt;
        // `mkntfs`/`wimlib` need a block device; `newfs_msdos` wants a character
        // (raw) one; `diskutil` accepts either.
        let windows_part = partition_node(&plan.device.id, if gpt { 2 } else { 1 });
        let esp_part = partition_node(&plan.device.id, 1);
        let esp_raw = raw_partition_node(&plan.device.id, 1);

        let mkntfs = self.tools.require(Tool::Mkntfs)?;
        let wimlib = self.tools.require(Tool::WimlibImagex)?;
        let bcd_sys = self.tools.require(Tool::BcdSys)?;

        // Preparing -----------------------------------------------------
        script.mark(Stage::Preparing);
        script.run(&Command::new(DISKUTIL).args(["unmountDisk", "force", disk.as_str()]));

        // Partitioning --------------------------------------------------
        script.mark(Stage::Partitioning);
        if gpt {
            // `diskutil` expects `Format Name Size` triplets and, for GPT,
            // implicitly creates the EFI System Partition as `s1`, so the
            // Windows volume lands on `s2`.
            script.run(&Command::new(DISKUTIL).args([
                "partitionDisk",
                disk.as_str(),
                "GPT",
                "MS-DOS",
                WINDOWS_LABEL,
                "R",
            ]));
        } else {
            let ms_sys = self.tools.require(Tool::MsSys)?;
            script.run(&Command::new(DISKUTIL).args([
                "partitionDisk",
                disk.as_str(),
                "MBR",
                "MS-DOS",
                WINDOWS_LABEL,
                "R",
            ]));
            script.run(&Command::new(DISKUTIL).args(["unmountDisk", "force", disk.as_str()]));
            script.run(&Command::new(ms_sys.display().to_string()).args(["-m", raw_disk.as_str()]));
        }
        script.run(&Command::new(DISKUTIL).args(["unmountDisk", "force", disk.as_str()]));

        // Formatting ----------------------------------------------------
        script.mark(Stage::Formatting);
        if gpt {
            // FAT32 requires clusters at least as large as the device's logical
            // sector (and >= 65525 of them). `newfs_msdos -c` takes sectors per
            // cluster. No `-v`: Rufus advises against labelling the ESP.
            let sectors_per_cluster = (plan.device.block_size / 512).max(1).to_string();
            script.run(&Command::new(NEWFS_MSDOS).args([
                "-F",
                "32",
                "-c",
                sectors_per_cluster.as_str(),
                esp_raw.as_str(),
            ]));
        }
        script.run(&Command::new(mkntfs.display().to_string()).args([
            "-f",
            "-L",
            WINDOWS_LABEL,
            windows_part.as_str(),
        ]));

        // Applying the image --------------------------------------------
        script.mark(Stage::ApplyingImage);
        let index = plan.image.edition_index.max(1).to_string();
        // Keep ACLs and file attributes: Rufus/DISM restore them, and a normal
        // install can fail to boot without them.
        script.run(&Command::new(wimlib.display().to_string()).args([
            "apply".to_owned(),
            install_image.display().to_string(),
            index,
            windows_part.clone(),
            "--include-invalid-names".to_owned(),
        ]));

        // Steam hardware drivers ----------------------------------------
        // `ntfscp` cannot create directories and there is no `ntfsmkdir`, so the
        // staged tree is captured into a small overlay WIM and applied on top of
        // the fresh NTFS volume. The overlay only contains the new
        // `\SteamDeckDrivers` folder, so no existing file is touched.
        if plan.options.drivers.is_enabled() {
            script.mark(Stage::InstallingDrivers);
            let drivers_wim = mounts.staging.join(DRIVERS_WIM);
            script.run(&Command::new(wimlib.display().to_string()).args([
                "capture".to_owned(),
                mounts.drivers_root.display().to_string(),
                drivers_wim.display().to_string(),
                "--compress=none".to_owned(),
                "--no-acls".to_owned(),
            ]));
            script.run(&Command::new(wimlib.display().to_string()).args([
                "apply".to_owned(),
                drivers_wim.display().to_string(),
                "1".to_owned(),
                windows_part.clone(),
                "--no-acls".to_owned(),
                "--include-invalid-names".to_owned(),
            ]));
        }

        // Offline Windows To Go configuration ---------------------------
        self.append_offline_config(&mut script, plan, &windows_part, mounts)?;

        // Bootloader ----------------------------------------------------
        script.mark(Stage::InstallingBootloader);
        script.raw(format!(
            "mkdir -p {} {}",
            shell_quote(&mounts.windows.display().to_string()),
            shell_quote(&mounts.esp.display().to_string())
        ));
        // The volume may be auto-mounted read-only already; ignore failures.
        script.allow_failure(&Command::new(DISKUTIL).args([
            "unmount",
            "force",
            windows_part.as_str(),
        ]));
        script.run(&Command::new(DISKUTIL).args([
            "mount",
            "-mountPoint",
            mounts.windows.to_string_lossy().as_ref(),
            windows_part.as_str(),
        ]));
        if gpt {
            script.allow_failure(
                &Command::new(DISKUTIL)
                    .args(["unmount", "force"])
                    .arg(esp_part.as_str()),
            );
            script.run(&Command::new(DISKUTIL).args([
                "mount",
                "-mountPoint",
                mounts.esp.to_string_lossy().as_ref(),
                esp_part.as_str(),
            ]));
            script.run(&Command::new(bcd_sys.display().to_string()).args([
                mounts.windows.to_string_lossy().as_ref(),
                "-s",
                mounts.esp.to_string_lossy().as_ref(),
            ]));
            copy_uefi_fallback(&mut script, mounts);
            script.run(
                &Command::new(DISKUTIL)
                    .arg("unmount")
                    .arg(mounts.esp.to_string_lossy().as_ref()),
            );
        } else {
            script.run(&Command::new(bcd_sys.display().to_string()).args([
                "-f",
                "BIOS",
                mounts.windows.to_string_lossy().as_ref(),
            ]));
        }
        script.run(
            &Command::new(DISKUTIL)
                .arg("unmount")
                .arg(mounts.windows.to_string_lossy().as_ref()),
        );

        // Verifying -----------------------------------------------------
        if plan.options.verify_after_write {
            script.mark(Stage::Verifying);
            script.run(
                &Command::new(wimlib.display().to_string())
                    .args(["verify".to_owned(), install_image.display().to_string()]),
            );
        }

        // Ejecting ------------------------------------------------------
        script.mark(Stage::Ejecting);
        script.run(&Command::new(DISKUTIL).arg("eject").arg(disk.as_str()));

        Ok(script.finish())
    }

    /// Patch the applied (still unmounted) NTFS volume so Windows treats it as a
    /// portable installation, plus optionally create a local account.
    fn append_offline_config(
        &self,
        script: &mut Script,
        plan: &WritePlan,
        windows_part: &str,
        mounts: &MountPoints,
    ) -> Result<()> {
        script.mark(Stage::Configuring);

        let ntfscat = self.tools.require(Tool::NtfsCat)?;
        let ntfscp = self.tools.require(Tool::NtfsCp)?;
        let hiveset = self.tools.require(Tool::HiveSet)?;

        let system_hive = mounts.staging.join("SYSTEM");
        let software_hive = mounts.staging.join("SOFTWARE");
        let unattend = mounts.staging.join("unattend.xml");

        // SYSTEM hive: WTG flag, SanPolicy, hibernation/fast-startup.
        // `hiveset` replaces a single value at a time, so shared keys such as
        // `\ControlSet001\Control` keep their unrelated values.
        extract_hive(script, &ntfscat, windows_part, SYSTEM_HIVE, &system_hive);
        for entry in SYSTEM_HIVE_VALUES {
            set_hive_value(script, &hiveset, &system_hive, entry);
        }
        restore_hive(script, &ntfscp, windows_part, &system_hive, SYSTEM_HIVE);

        // SOFTWARE hive: `OOBE\BypassNRO=1` enables the offline account path.
        if plan.options.bypass_network {
            extract_hive(
                script,
                &ntfscat,
                windows_part,
                SOFTWARE_HIVE,
                &software_hive,
            );
            for entry in SOFTWARE_HIVE_VALUES {
                set_hive_value(script, &hiveset, &software_hive, entry);
            }
            restore_hive(script, &ntfscp, windows_part, &software_hive, SOFTWARE_HIVE);
        }

        // `unattend.xml` disables the online-account screens, creates the local
        // administrator and runs `powercfg /h off`. Fall back to Sysprep if the
        // Panther directory is missing in the image.
        let primary = Command::new(ntfscp.display().to_string())
            .args([
                "-f".to_owned(),
                windows_part.to_owned(),
                unattend.display().to_string(),
                UNATTEND_PANTHER.to_owned(),
            ])
            .shell();
        let fallback = Command::new(ntfscp.display().to_string())
            .args([
                "-f".to_owned(),
                windows_part.to_owned(),
                unattend.display().to_string(),
                UNATTEND_SYSPREP.to_owned(),
            ])
            .shell();
        script.raw(format!("if ! {primary}; then {fallback}; fi"));

        Ok(())
    }
}

/// A single registry value written into an offline hive.
struct HiveValue {
    path: &'static str,
    name: &'static str,
    value: u32,
}

/// Windows To Go keys written into `SYSTEM` (see `TASK.md` §1).
const SYSTEM_HIVE_VALUES: &[HiveValue] = &[
    HiveValue {
        path: r"\ControlSet001\Control",
        name: "PortableOperatingSystem",
        value: 1,
    },
    HiveValue {
        path: r"\ControlSet001\Control",
        name: "BootDriverFlags",
        value: 0x14,
    },
    HiveValue {
        path: r"\ControlSet001\Services\partmgr\Parameters",
        name: "SanPolicy",
        value: 4,
    },
    HiveValue {
        path: r"\ControlSet001\Control\Session Manager\Power",
        name: "HiberbootEnabled",
        value: 0,
    },
    HiveValue {
        path: r"\ControlSet001\Control\Session Manager\Power",
        name: "DisableSelectiveSuspend",
        value: 1,
    },
    HiveValue {
        path: r"\ControlSet001\Control\Power",
        name: "HibernateEnabled",
        value: 0,
    },
    HiveValue {
        path: r"\ControlSet001\Control\Power",
        name: "HibernateEnabledDefault",
        value: 0,
    },
];

/// OOBE key written into `SOFTWARE`.
const SOFTWARE_HIVE_VALUES: &[HiveValue] = &[HiveValue {
    path: r"\Microsoft\Windows\CurrentVersion\OOBE",
    name: "BypassNRO",
    value: 1,
}];

/// Copy a hive off the unmounted NTFS partition into the staging directory.
fn extract_hive(
    script: &mut Script,
    ntfscat: &Path,
    windows_part: &str,
    source: &str,
    destination: &Path,
) {
    script.raw(format!(
        "{} {} {} > {}",
        shell_quote(&ntfscat.display().to_string()),
        shell_quote(windows_part),
        shell_quote(source),
        shell_quote(&destination.display().to_string()),
    ));
}

/// Write one value into a staged hive, preserving all other values.
fn set_hive_value(script: &mut Script, hiveset: &Path, hive: &Path, entry: &HiveValue) {
    script.run(&Command::new(hiveset.display().to_string()).args([
        hive.display().to_string(),
        entry.path.to_owned(),
        entry.name.to_owned(),
        format!("dword:{:#x}", entry.value),
    ]));
}

/// Write the patched hive back onto the unmounted NTFS partition.
fn restore_hive(
    script: &mut Script,
    ntfscp: &Path,
    windows_part: &str,
    source: &Path,
    destination: &str,
) {
    script.run(&Command::new(ntfscp.display().to_string()).args([
        "-f".to_owned(),
        windows_part.to_owned(),
        source.display().to_string(),
        destination.to_owned(),
    ]));
}

impl Default for RealWriter {
    fn default() -> Self {
        Self::system()
    }
}

impl WindowsToGoWriter for RealWriter {
    fn write(&self, plan: &WritePlan, sink: &dyn ProgressSink) -> Result<()> {
        plan.validate()?;
        self.check_cancelled()?;
        if !is_valid_device_id(&plan.device.id) {
            return Err(CoreError::DeviceNotFound(plan.device.id.clone()));
        }
        self.preflight(plan)?;
        self.ensure_capacity(plan, 0)?;

        let mounts = MountPoints::new()?;
        let driver_bytes = self.prepare_drivers(plan, &mounts, sink)?;
        self.check_cancelled()?;
        self.ensure_capacity(plan, driver_bytes)?;

        let (install_image, _mount) = self.resolve_install_image(plan)?;
        mounts.write_config(plan)?;
        let script = self.build_script(plan, &install_image, &mounts)?;

        self.check_cancelled()?;
        sink.report(ProgressEvent::new(
            Stage::Preparing,
            Stage::Preparing.start_fraction(),
        ));

        let mut current = Stage::Preparing;
        let mut pulse = 0.0_f32;
        let command = Command::new("/bin/sh").arg("-c").arg(script).privileged();
        self.runner.run_streaming(&command, &mut |line| {
            if let Some(token) = line.strip_prefix(SENTINEL) {
                if let Some(stage) = Stage::from_token(token.trim()) {
                    current = stage;
                    pulse = 0.0;
                    sink.report(ProgressEvent::new(stage, stage.start_fraction()));
                }
            } else if current == Stage::ApplyingImage {
                pulse = (pulse + 0.004).min(0.95);
                let start = current.start_fraction();
                let end = next_stage_fraction(current);
                sink.report(
                    ProgressEvent::new(current, start + (end - start) * pulse).with_detail(line),
                );
            }
        })?;

        sink.report(ProgressEvent::new(Stage::Finished, 1.0));
        Ok(())
    }

    fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
        self.runner.cancel();
    }
}

/// Buffered (block) device node: `/dev/diskN sX`.
///
/// Required by `mkntfs` and `wimlib-imagex`, which refuse character devices.
fn partition_node(device: &str, index: u32) -> String {
    format!("/dev/{device}s{index}")
}

/// Raw (character) device node: `/dev/rdiskN sX`.
///
/// `newfs_msdos` warns unless it is given a character device.
fn raw_partition_node(device: &str, index: u32) -> String {
    format!("/dev/r{device}s{index}")
}

fn next_stage_fraction(stage: Stage) -> f32 {
    let index = Stage::ALL.iter().position(|candidate| *candidate == stage);
    match index.and_then(|index| Stage::ALL.get(index + 1)) {
        Some(next) => next.start_fraction(),
        None => 1.0,
    }
}

fn copy_uefi_fallback(script: &mut Script, mounts: &MountPoints) {
    let source = mounts
        .windows
        .join("Windows")
        .join("Boot")
        .join("EFI")
        .join("bootmgfw.efi");
    let target_dir = mounts.esp.join("EFI").join("BOOT");
    let target = target_dir.join("BOOTX64.EFI");
    script.raw(format!(
        "if [ -f {source} ]; then mkdir -p {dir}; cp {source} {target}; fi",
        source = shell_quote(&source.display().to_string()),
        dir = shell_quote(&target_dir.display().to_string()),
        target = shell_quote(&target.display().to_string()),
    ));
}

/// Temporary mount points for the Windows volume, the ESP and the staging files.
struct MountPoints {
    windows: PathBuf,
    esp: PathBuf,
    staging: PathBuf,
    /// Directory captured into the overlay WIM; contains `SteamDeckDrivers/`.
    drivers_root: PathBuf,
    /// Downloads of the official ZIP packages.
    drivers_downloads: PathBuf,
    _windows_dir: tempfile::TempDir,
    _esp_dir: tempfile::TempDir,
    _staging_dir: tempfile::TempDir,
    _drivers_dir: tempfile::TempDir,
}

impl MountPoints {
    fn new() -> Result<Self> {
        let windows_dir =
            tempfile::TempDir::new().map_err(|error| CoreError::Mount(error.to_string()))?;
        let esp_dir =
            tempfile::TempDir::new().map_err(|error| CoreError::Mount(error.to_string()))?;
        let staging_dir =
            tempfile::TempDir::new().map_err(|error| CoreError::Mount(error.to_string()))?;
        let drivers_dir =
            tempfile::TempDir::new().map_err(|error| CoreError::Mount(error.to_string()))?;
        let windows = windows_dir.path().join("windows");
        let esp = esp_dir.path().join("efi");
        let drivers_root = drivers_dir.path().join("root");
        let drivers_downloads = drivers_dir.path().join("downloads");
        Ok(Self {
            windows,
            esp,
            staging: staging_dir.path().to_path_buf(),
            drivers_root,
            drivers_downloads,
            _windows_dir: windows_dir,
            _esp_dir: esp_dir,
            _staging_dir: staging_dir,
            _drivers_dir: drivers_dir,
        })
    }

    /// Materialise the `unattend.xml` that the privileged script copies onto
    /// the image.
    fn write_config(&self, plan: &WritePlan) -> Result<()> {
        std::fs::write(
            self.staging.join("unattend.xml"),
            unattend_xml(&plan.options, plan.image.architecture.as_deref()),
        )
        .map_err(|error| CoreError::Mount(error.to_string()))
    }
}

/// Map a wimlib architecture name to the `processorArchitecture` used in answer
/// files.
fn processor_architecture(architecture: Option<&str>) -> &'static str {
    match architecture {
        Some("x86_64") => "amd64",
        Some("ARM64") => "arm64",
        Some("x86") => "x86",
        _ => "amd64",
    }
}

/// Answer file written to `\Windows\Panther\unattend.xml`.
fn unattend_xml(options: &WriteOptions, architecture: Option<&str>) -> String {
    let mut commands = String::from(
        "<SynchronousCommand wcm:action=\"add\">\n\
         <Order>1</Order>\n\
         <CommandLine>powercfg /h off</CommandLine>\n\
         <Description>Disable hibernation and fast startup</Description>\n\
         </SynchronousCommand>",
    );
    if options.drivers.is_enabled() {
        // `pnputil` installs every INF below the staged folder. A failure must
        // not abort OOBE, hence the trailing `exit /b 0`.
        commands.push_str(
            "\n<SynchronousCommand wcm:action=\"add\">\n\
             <Order>2</Order>\n\
             <CommandLine>cmd /c \"pnputil /add-driver C:\\SteamDeckDrivers\\*.inf /subdirs /install &amp; exit /b 0\"</CommandLine>\n\
             <Description>Install Steam hardware drivers</Description>\n\
             </SynchronousCommand>",
        );
    }

    let accounts = if options.create_local_account {
        format!(
            "<UserAccounts>\n\
             <LocalAccounts>\n\
             <LocalAccount wcm:action=\"add\">\n\
             <Name>{name}</Name>\n\
             <Group>Administrators</Group>\n\
             <Password>\n\
             <Value>{password}</Value>\n\
             <PlainText>true</PlainText>\n\
             </Password>\n\
             </LocalAccount>\n\
             </LocalAccounts>\n\
             </UserAccounts>",
            name = xml_escape(&options.local_username),
            password = xml_escape(&options.local_password),
        )
    } else {
        String::new()
    };

    format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<unattend xmlns="urn:schemas-microsoft-com:unattend">
  <settings pass="oobeSystem">
    <component name="Microsoft-Windows-Shell-Setup" processorArchitecture="{architecture}" publicKeyToken="31bf3856ad364e35" language="neutral" versionScope="nonSxS" xmlns:wcm="http://schemas.microsoft.com/WMIConfig/2002/State" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance">
      <OOBE>
        <HideOnlineAccountScreens>true</HideOnlineAccountScreens>
        <HideWirelessSetupInOOBE>true</HideWirelessSetupInOOBE>
        <HideEULAPage>true</HideEULAPage>
        <ProtectYourPC>3</ProtectYourPC>
      </OOBE>
      {accounts}
      <FirstLogonCommands>
      {commands}
      </FirstLogonCommands>
    </component>
  </settings>
</unattend>
"#,
        accounts = accounts,
        commands = commands,
        architecture = processor_architecture(architecture),
    )
}

/// Escape a value for inclusion in XML text or attribute values.
fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// Whether a path looks like a ZIP archive.
fn is_zip(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("zip"))
}

/// File name of a path, falling back to the full path.
fn file_name(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    )
}

/// Recursively collect the files under `dir` whose extension matches.
fn collect_files_with_extension(dir: &Path, extension: &str, out: &mut Vec<PathBuf>) -> Result<()> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(CoreError::Io(error)),
    };
    for entry in entries {
        let entry = entry.map_err(CoreError::Io)?;
        let path = entry.path();
        if entry.file_type().map_err(CoreError::Io)?.is_dir() {
            collect_files_with_extension(&path, extension, out)?;
        } else if path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case(extension))
        {
            out.push(path);
        }
    }
    Ok(())
}

/// Recursively copy a directory tree.
fn copy_dir_all(source: &Path, destination: &Path) -> Result<()> {
    std::fs::create_dir_all(destination).map_err(CoreError::Io)?;
    for entry in std::fs::read_dir(source).map_err(CoreError::Io)? {
        let entry = entry.map_err(CoreError::Io)?;
        let from = entry.path();
        let to = destination.join(entry.file_name());
        if entry.file_type().map_err(CoreError::Io)?.is_dir() {
            copy_dir_all(&from, &to)?;
        } else {
            std::fs::copy(&from, &to).map_err(CoreError::Io)?;
        }
    }
    Ok(())
}

/// Total size of the files in a directory tree.
fn dir_size(path: &Path) -> u64 {
    let Ok(metadata) = std::fs::metadata(path) else {
        return 0;
    };
    if metadata.is_file() {
        return metadata.len();
    }
    let Ok(entries) = std::fs::read_dir(path) else {
        return 0;
    };
    entries
        .filter_map(|entry| entry.ok())
        .map(|entry| dir_size(&entry.path()))
        .sum()
}

/// First `.inf` file found under `dir`, if any.
fn find_first_inf(dir: &Path) -> Option<PathBuf> {
    let mut files = Vec::new();
    collect_files_with_extension(dir, "inf", &mut files).ok()?;
    files.into_iter().next()
}

/// Accumulates a privileged shell script.
struct Script {
    lines: Vec<String>,
}

impl Script {
    fn new() -> Self {
        Self {
            lines: vec!["set -e".to_owned()],
        }
    }

    fn mark(&mut self, stage: Stage) {
        self.lines
            .push(format!("echo '{SENTINEL}{}'", stage.token()));
    }

    fn run(&mut self, command: &Command) {
        self.lines.push(command.shell());
    }

    fn allow_failure(&mut self, command: &Command) {
        self.lines.push(format!("{} || true", command.shell()));
    }

    fn raw(&mut self, line: impl Into<String>) {
        self.lines.push(line.into());
    }

    fn finish(self) -> String {
        self.lines.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::device::{BusType, DiskDevice};
    use crate::core::image::{ImageEdition, WindowsImage};
    use crate::core::options::WriteOptions;
    use crate::core::progress::{NullProgressSink, ProgressEvent};
    use std::sync::Mutex;

    #[derive(Default)]
    struct RecordingRunner {
        commands: Mutex<Vec<Command>>,
    }

    impl CommandRunner for RecordingRunner {
        fn run(&self, command: &Command) -> Result<String> {
            self.commands.lock().unwrap().push(command.clone());
            Ok(String::new())
        }

        fn run_streaming(&self, command: &Command, _on_line: &mut dyn FnMut(&str)) -> Result<()> {
            self.commands.lock().unwrap().push(command.clone());
            Ok(())
        }
    }

    #[derive(Default)]
    struct CollectingSink(Mutex<Vec<ProgressEvent>>);

    impl ProgressSink for CollectingSink {
        fn report(&self, event: ProgressEvent) {
            self.0.lock().unwrap().push(event);
        }
    }

    fn tools_dir(with_ms_sys: bool) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        for name in [
            "wimlib-imagex",
            "mkntfs",
            "ntfscat",
            "ntfscp",
            "hiveset",
            "hivexsh",
            "bcd-sys",
        ] {
            std::fs::write(dir.path().join(name), b"#!/bin/sh\n").unwrap();
        }
        if with_ms_sys {
            std::fs::write(dir.path().join("ms-sys"), b"#!/bin/sh\n").unwrap();
        }
        dir
    }

    fn image() -> WindowsImage {
        WindowsImage {
            path: "/tmp/win11.wim".into(),
            format: ImageFormat::Wim,
            size_bytes: 6_000_000_000,
            edition: Some("Windows 11 Pro".into()),
            architecture: Some("x86_64".into()),
            edition_index: 2,
            version: None,
            editions: vec![ImageEdition {
                index: 2,
                name: "Windows 11 Pro".into(),
                description: None,
                architecture: Some("x86_64".into()),
                size_bytes: Some(11_000_000_000),
                version: None,
            }],
        }
    }

    fn device() -> DiskDevice {
        DiskDevice {
            id: "disk9".into(),
            name: "SanDisk".into(),
            size_bytes: 64_000_000_000,
            block_size: 512,
            bus: BusType::Usb,
            is_removable: true,
            is_internal: false,
            partition_scheme: None,
            volumes: vec![],
        }
    }

    fn plan(scheme: PartitionScheme) -> WritePlan {
        WritePlan {
            device: device(),
            image: image(),
            options: WriteOptions {
                partition_scheme: scheme,
                uefi: scheme == PartitionScheme::Gpt,
                ..WriteOptions::default()
            },
        }
    }

    fn run_writer(
        plan: &WritePlan,
        dir: &tempfile::TempDir,
    ) -> (Result<()>, Vec<Command>, Vec<ProgressEvent>) {
        let runner = Arc::new(RecordingRunner::default());
        let writer = RealWriter::with_runner(
            Arc::clone(&runner) as Arc<dyn CommandRunner>,
            ToolLocator::with_search_dirs(vec![dir.path().to_path_buf()]),
        );
        let sink = CollectingSink::default();
        let result = writer.write(plan, &sink);
        let commands = runner.commands.lock().unwrap().clone();
        let events = sink.0.lock().unwrap().clone();
        (result, commands, events)
    }

    fn script_of(commands: &[Command]) -> String {
        commands
            .iter()
            .find(|command| command.program == "/bin/sh")
            .map(|command| command.args.join(" "))
            .unwrap_or_default()
    }

    #[test]
    fn gpt_script_contains_all_stages() {
        let dir = tools_dir(false);
        let (result, commands, events) = run_writer(&plan(PartitionScheme::Gpt), &dir);
        result.unwrap();

        let script = script_of(&commands);
        for expected in [
            "partitionDisk",
            "GPT",
            "newfs_msdos",
            "mkntfs",
            "wimlib-imagex",
            "apply",
            "/dev/disk9s2",
            "/dev/rdisk9s1",
            "ntfscat",
            "hiveset",
            "ntfscp",
            "unattend.xml",
            "bcd-sys",
            "BOOTX64.EFI",
            "eject",
        ] {
            assert!(
                script.contains(expected),
                "script missing `{expected}`:\n{script}"
            );
        }
        // `diskutil` triplets are `Format Name Size`; GPT gets an implicit ESP.
        assert!(
            script.contains("'GPT' 'MS-DOS' 'WINDOWS' 'R'"),
            "unexpected partitionDisk arguments:\n{script}"
        );
        // `mkntfs`/`wimlib` require block device nodes; `newfs_msdos` a raw one.
        assert!(
            script.contains("'-f' '-L' 'WINDOWS' '/dev/disk9s2'"),
            "mkntfs should target the block device:\n{script}"
        );
        assert!(
            script.contains("'/dev/rdisk9s1'"),
            "newfs_msdos should target the raw device:\n{script}"
        );
        // 512-byte sectors => one sector per ESP cluster, and no ESP label.
        assert!(
            script.contains("'-c' '1'"),
            "newfs_msdos should use a sector-sized cluster:\n{script}"
        );
        assert!(
            !script.contains("'-v'"),
            "the ESP must not be labelled:\n{script}"
        );
        // Rufus/DISM keep ACLs and attributes.
        assert!(!script.contains("--no-acls"), "{script}");
        assert!(!script.contains("--no-attributes"), "{script}");
        assert!(script.contains("::m2w:configuring"), "{script}");
        assert_eq!(events.last().unwrap().stage, Stage::Finished);
    }

    #[test]
    fn esp_cluster_matches_4k_sector_size() {
        let dir = tools_dir(false);
        let mut plan = plan(PartitionScheme::Gpt);
        plan.device.block_size = 4096;
        let (result, commands, _) = run_writer(&plan, &dir);
        result.unwrap();

        let script = script_of(&commands);
        assert!(
            script.contains("'-c' '8'"),
            "4Kn sectors need an 8-sector (4 KiB) cluster:\n{script}"
        );
    }

    #[test]
    fn offline_config_sets_wtg_keys_with_hiveset() {
        let dir = tools_dir(false);
        let (result, commands, _) = run_writer(&plan(PartitionScheme::Gpt), &dir);
        result.unwrap();

        let script = script_of(&commands);
        for expected in [
            "hiveset",
            r"\ControlSet001\Control'",
            "PortableOperatingSystem",
            "BootDriverFlags",
            "dword:0x14",
            r"\ControlSet001\Services\partmgr\Parameters'",
            "SanPolicy",
            r"\ControlSet001\Control\Session Manager\Power'",
            "HiberbootEnabled",
            "DisableSelectiveSuspend",
            r"\ControlSet001\Control\Power'",
            "HibernateEnabled",
            "HibernateEnabledDefault",
            r"\Microsoft\Windows\CurrentVersion\OOBE'",
            "BypassNRO",
        ] {
            assert!(
                script.contains(expected),
                "offline config missing `{expected}`:\n{script}"
            );
        }
        // `hivexsh`'s bulk `setval` wipes every other value at the node, so the
        // offline config must use the single-value `hiveset` helper instead.
        assert!(!script.contains("setval"), "{script}");
    }

    #[test]
    fn unattend_creates_local_account_and_disables_hibernation() {
        let options = WriteOptions {
            local_username: "deck".to_owned(),
            local_password: "s3<cret&".to_owned(),
            ..WriteOptions::default()
        };
        let xml = unattend_xml(&options, Some("x86_64"));
        assert!(xml.contains("<Name>deck</Name>"));
        assert!(xml.contains("<Value>s3&lt;cret&amp;</Value>"));
        assert!(xml.contains("<Group>Administrators</Group>"));
        assert!(xml.contains("<HideOnlineAccountScreens>true</HideOnlineAccountScreens>"));
        assert!(xml.contains("<CommandLine>powercfg /h off</CommandLine>"));
        assert!(xml.contains("processorArchitecture=\"amd64\""));
    }

    #[test]
    fn unattend_omits_account_when_disabled() {
        let options = WriteOptions {
            create_local_account: false,
            ..WriteOptions::default()
        };
        let xml = unattend_xml(&options, None);
        assert!(!xml.contains("<LocalAccount"));
        assert!(xml.contains("<HideOnlineAccountScreens>true</HideOnlineAccountScreens>"));
        assert!(xml.contains("processorArchitecture=\"amd64\""));
    }

    #[test]
    fn unattend_installs_drivers_when_enabled() {
        let options = WriteOptions {
            drivers: DriverSource::Auto(drivers::SteamHardware::DeckOled),
            ..WriteOptions::default()
        };
        let xml = unattend_xml(&options, Some("x86_64"));
        assert!(xml.contains("pnputil /add-driver C:\\SteamDeckDrivers\\*.inf /subdirs /install"));
        assert!(xml.contains("Install Steam hardware drivers"));
        assert!(xml.contains("<CommandLine>powercfg /h off</CommandLine>"));
    }

    #[test]
    fn unattend_omits_driver_command_when_disabled() {
        let xml = unattend_xml(&WriteOptions::default(), Some("x86_64"));
        assert!(!xml.contains("pnputil"));
    }

    #[test]
    fn local_drivers_are_captured_and_installed() {
        let dir = tools_dir(false);
        let drivers_dir = tempfile::tempdir().unwrap();
        std::fs::write(drivers_dir.path().join("qcwlan64.inf"), b"; fake inf\n").unwrap();
        std::fs::create_dir(drivers_dir.path().join("nested")).unwrap();
        std::fs::write(
            drivers_dir.path().join("nested").join("extra.inf"),
            b"; x\n",
        )
        .unwrap();

        let mut plan = plan(PartitionScheme::Gpt);
        plan.options.drivers = DriverSource::Local(vec![drivers_dir.path().to_path_buf()]);
        let (result, commands, _) = run_writer(&plan, &dir);
        result.unwrap();

        let script = script_of(&commands);
        assert!(script.contains("::m2w:installing_drivers"), "{script}");
        assert!(script.contains("capture"), "{script}");
        assert!(script.contains("drivers.wim"), "{script}");
        assert!(script.contains("--compress=none"), "{script}");
    }

    #[test]
    fn empty_local_driver_source_is_rejected() {
        let dir = tools_dir(false);
        let mut plan = plan(PartitionScheme::Gpt);
        plan.options.drivers = DriverSource::Local(Vec::new());
        let (result, commands, _) = run_writer(&plan, &dir);
        assert!(matches!(result, Err(CoreError::InvalidConfig(_))));
        assert!(commands.is_empty());
    }

    #[test]
    fn local_driver_source_without_inf_is_rejected() {
        let dir = tools_dir(false);
        let drivers_dir = tempfile::tempdir().unwrap();
        std::fs::write(drivers_dir.path().join("readme.txt"), b"no drivers here").unwrap();
        let mut plan = plan(PartitionScheme::Gpt);
        plan.options.drivers = DriverSource::Local(vec![drivers_dir.path().to_path_buf()]);
        let (result, commands, _) = run_writer(&plan, &dir);
        assert!(matches!(result, Err(CoreError::InvalidConfig(_))));
        assert!(commands.is_empty());
    }

    #[test]
    fn mbr_script_uses_ms_sys() {
        let dir = tools_dir(true);
        let (result, commands, _) = run_writer(&plan(PartitionScheme::Mbr), &dir);
        result.unwrap();

        let script = script_of(&commands);
        assert!(script.contains("MBR"), "{script}");
        assert!(script.contains("ms-sys"), "{script}");
        assert!(script.contains("BIOS"), "{script}");
        // `diskutil` triplets are `Format Name Size`.
        assert!(
            script.contains("'MBR' 'MS-DOS' 'WINDOWS' 'R'"),
            "unexpected partitionDisk arguments:\n{script}"
        );
        // `ms-sys` writes the whole raw disk; `mkntfs` needs the block slice.
        assert!(script.contains("'-m' '/dev/rdisk9'"), "{script}");
        assert!(
            script.contains("'-f' '-L' 'WINDOWS' '/dev/disk9s1'"),
            "mkntfs should target the block device:\n{script}"
        );
    }

    #[test]
    fn missing_tools_abort_before_doing_anything() {
        let dir = tempfile::tempdir().unwrap();
        let (result, commands, _) = run_writer(&plan(PartitionScheme::Gpt), &dir);
        assert!(matches!(result, Err(CoreError::ToolMissing(_))));
        assert!(commands.is_empty());
    }

    #[test]
    fn oversized_image_is_rejected() {
        let dir = tools_dir(false);
        let mut plan = plan(PartitionScheme::Gpt);
        plan.device.size_bytes = 8_000_000_000;
        let (result, commands, _) = run_writer(&plan, &dir);
        assert!(matches!(result, Err(CoreError::InvalidConfig(_))));
        assert!(commands.is_empty());
    }

    #[test]
    fn cancellation_stops_the_write() {
        let dir = tools_dir(false);
        let runner = Arc::new(RecordingRunner::default());
        let writer = RealWriter::with_runner(
            Arc::clone(&runner) as Arc<dyn CommandRunner>,
            ToolLocator::with_search_dirs(vec![dir.path().to_path_buf()]),
        );
        writer.cancel();
        let result = writer.write(&plan(PartitionScheme::Gpt), &NullProgressSink);
        assert!(matches!(result, Err(CoreError::Cancelled)));
    }
}
