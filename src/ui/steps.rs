//! Rendering of each wizard step.

use egui::{RichText, Ui};

use crate::app::{Mac2WinApp, Step};
use crate::core::device::BusType;
use crate::core::drivers::{DriverSource, SteamHardware};
use crate::core::options::{FileSystem, PartitionScheme};
use crate::core::progress::Stage;
use crate::i18n::I18n;
use crate::ui::{macos, widgets};

fn bus_label(t: &I18n, bus: BusType) -> &'static str {
    t.t(match bus {
        BusType::Usb => "bus.usb",
        BusType::Thunderbolt => "bus.thunderbolt",
        BusType::FireWire => "bus.firewire",
        BusType::DiskImage => "bus.disk_image",
        BusType::Internal => "bus.internal",
        BusType::Unknown => "bus.unknown",
    })
}

pub fn show(app: &mut Mac2WinApp, ui: &mut Ui) {
    match app.step() {
        Step::Welcome => welcome(app, ui),
        Step::Image => image(app, ui),
        Step::Device => device(app, ui),
        Step::Options => options(app, ui),
        Step::Confirm => confirm(app, ui),
        Step::Writing => writing(app, ui),
        Step::Done => done(app, ui),
        Step::Failed => failed(app, ui),
    }
}

fn welcome(app: &mut Mac2WinApp, ui: &mut Ui) {
    let t = *app.i18n();
    widgets::section_heading(ui, t.t("step.welcome.title"));
    ui.add(egui::Label::new(t.t("step.welcome.body")).wrap());
    ui.add_space(12.0);
    macos::alert(ui, macos::AlertKind::Warning, t.t("app.admin_notice"));

    let missing = app.missing_tools();
    if !missing.is_empty() {
        ui.add_space(8.0);
        macos::alert(ui, macos::AlertKind::Error, t.t("app.tools_missing"));
        for tool in missing {
            widgets::bullet(ui, tool.binary_name());
        }
    }
}

fn image(app: &mut Mac2WinApp, ui: &mut Ui) {
    let t = *app.i18n();
    widgets::section_heading(ui, t.t("step.image.title"));
    ui.label(t.t("step.image.body"));
    ui.add_space(8.0);

    if macos::push_button(
        ui,
        t.t("step.image.choose"),
        macos::ButtonKind::Secondary,
        true,
    )
    .clicked()
    {
        if let Some(path) = pick_image_file() {
            if let Err(error) = app.select_image(path) {
                app.set_error(error.to_string());
            }
        }
    }

    widgets::spaced_separator(ui);
    match app.image().cloned() {
        Some(image) => {
            macos::card(ui, |ui| {
                widgets::key_value(
                    ui,
                    t.t("step.image.selected"),
                    &image.path.display().to_string(),
                );
                widgets::key_value(ui, t.t("label.format"), &format!("{:?}", image.format));
                widgets::key_value(ui, t.t("label.size"), &image.size_human());
                widgets::key_value(
                    ui,
                    t.t("label.architecture"),
                    image.architecture.as_deref().unwrap_or("—"),
                );
                if let Some(version) = image.version {
                    widgets::key_value(
                        ui,
                        t.t("label.version"),
                        &format!("{}.{}.{}", version.major, version.minor, version.build),
                    );
                }

                if image.editions.len() <= 1 {
                    widgets::key_value(
                        ui,
                        t.t("label.edition"),
                        image.edition.as_deref().unwrap_or("—"),
                    );
                }
            });

            if image.editions.len() > 1 {
                ui.add_space(8.0);
                ui.label(t.t("step.image.edition"));
                let current = image.edition_index;
                let mut selected = current;
                egui::ComboBox::from_id_salt("edition_selector")
                    .selected_text(image.edition.clone().unwrap_or_else(|| "—".to_owned()))
                    .width(ui.available_width())
                    .show_ui(ui, |ui| {
                        for edition in &image.editions {
                            let label = edition
                                .description
                                .clone()
                                .filter(|description| description != &edition.name)
                                .map_or_else(
                                    || edition.name.clone(),
                                    |description| format!("{} ({description})", edition.name),
                                );
                            ui.selectable_value(&mut selected, edition.index, label);
                        }
                    });
                if selected != current {
                    app.select_edition(selected);
                }
            }
        }
        None => {
            let color = ui.visuals().weak_text_color();
            widgets::notice(ui, t.t("step.image.none"), color);
        }
    }
}

fn device(app: &mut Mac2WinApp, ui: &mut Ui) {
    let t = *app.i18n();
    widgets::section_heading(ui, t.t("step.device.title"));
    ui.label(t.t("step.device.body"));
    ui.add_space(8.0);

    if macos::push_button(ui, t.t("nav.refresh"), macos::ButtonKind::Secondary, true).clicked() {
        app.scan_devices();
    }

    let targets: Vec<_> = app
        .devices()
        .iter()
        .filter(|device| device.is_eligible_target())
        .cloned()
        .collect();

    widgets::spaced_separator(ui);
    if targets.is_empty() {
        macos::alert(ui, macos::AlertKind::Warning, t.t("step.device.empty"));
        return;
    }

    for device in targets {
        let selected = app.selected_device_id() == Some(device.id.as_str());
        let label = RichText::new(format!(
            "{}   ·   {}   ·   {}",
            device.name,
            device.size_human(),
            bus_label(&t, device.bus)
        ));
        if ui.selectable_label(selected, label).clicked() {
            app.select_device(&device.id);
        }

        if selected {
            ui.add_space(4.0);
            macos::card(ui, |ui| {
                widgets::key_value(ui, t.t("label.name"), &device.name);
                widgets::key_value(ui, t.t("label.size"), &device.size_human());
                widgets::key_value(
                    ui,
                    t.t("label.scheme"),
                    device.partition_scheme.as_deref().unwrap_or("—"),
                );
                if !device.volumes.is_empty() {
                    ui.add_space(4.0);
                    ui.label(RichText::new(t.t("label.volumes")).weak());
                    for volume in &device.volumes {
                        let name = volume.name.as_deref().unwrap_or("—");
                        let mount = volume.mount_point.as_deref().unwrap_or("—");
                        widgets::bullet(ui, &format!("{name} ({mount})"));
                    }
                }
            });
        }
    }
}

fn options(app: &mut Mac2WinApp, ui: &mut Ui) {
    let t = *app.i18n();
    widgets::section_heading(ui, t.t("step.options.title"));

    macos::checkbox(ui, &mut app.options_mut().uefi, t.t("options.uefi"));
    widgets::hint(ui, t.t("options.uefi_hint"));
    ui.add_space(6.0);

    ui.label(t.t("label.partition_scheme"));
    let mut scheme = app.options().partition_scheme;
    let choices = [
        (PartitionScheme::Gpt, t.t("options.gpt")),
        (PartitionScheme::Mbr, t.t("options.mbr")),
    ];
    if macos::segmented(ui, &mut scheme, &choices) {
        app.options_mut().partition_scheme = scheme;
    }
    ui.add_space(6.0);

    macos::checkbox(
        ui,
        &mut app.options_mut().skip_size_check,
        t.t("options.skip_size_check"),
    );
    widgets::hint(ui, t.t("options.skip_size_check_hint"));
    ui.add_space(6.0);

    macos::checkbox(
        ui,
        &mut app.options_mut().verify_after_write,
        t.t("options.verify"),
    );
    widgets::hint(ui, t.t("options.verify_hint"));
    ui.add_space(6.0);

    driver_options(app, ui);

    macos::checkbox(
        ui,
        &mut app.options_mut().create_local_account,
        t.t("options.local_account"),
    );
    widgets::hint(ui, t.t("options.local_account_hint"));
    if app.options().create_local_account {
        ui.horizontal(|ui| {
            ui.label(t.t("options.local_username"));
            ui.text_edit_singleline(&mut app.options_mut().local_username);
        });
        ui.horizontal(|ui| {
            ui.label(t.t("options.local_password"));
            ui.add(
                egui::TextEdit::singleline(&mut app.options_mut().local_password).password(true),
            );
        });
    }

    ui.add_space(8.0);
    if let Err(error) = app.options().validate() {
        macos::alert(ui, macos::AlertKind::Error, &error.to_string());
    }
}

fn driver_options(app: &mut Mac2WinApp, ui: &mut Ui) {
    let t = *app.i18n();
    ui.label(t.t("options.drivers"));

    let current = app.options().drivers.clone();
    let local_entry = match &current {
        DriverSource::Local(paths) => DriverSource::Local(paths.clone()),
        _ => DriverSource::Local(Vec::new()),
    };
    let mut selected = current.clone();
    egui::ComboBox::from_id_salt("driver_source")
        .selected_text(driver_source_label(&t, &current))
        .width(ui.available_width())
        .show_ui(ui, |ui| {
            ui.selectable_value(
                &mut selected,
                DriverSource::None,
                t.t("options.drivers.none"),
            );
            for hardware in SteamHardware::ALL {
                ui.selectable_value(
                    &mut selected,
                    DriverSource::Auto(hardware),
                    t.t(hardware.key()),
                );
            }
            ui.selectable_value(&mut selected, local_entry, t.t("options.drivers.local"));
        });

    if selected != current {
        app.options_mut().drivers = selected;
    }

    if let DriverSource::Local(paths) = &mut app.options_mut().drivers {
        ui.horizontal(|ui| {
            if macos::push_button(
                ui,
                t.t("options.drivers.add_files"),
                macos::ButtonKind::Secondary,
                true,
            )
            .clicked()
            {
                if let Some(files) = rfd::FileDialog::new()
                    .add_filter("ZIP archives", &["zip"])
                    .pick_files()
                {
                    paths.extend(files);
                }
            }
            if macos::push_button(
                ui,
                t.t("options.drivers.add_folder"),
                macos::ButtonKind::Secondary,
                true,
            )
            .clicked()
            {
                if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                    paths.push(folder);
                }
            }
        });

        let mut remove = None;
        for (index, path) in paths.iter().enumerate() {
            ui.horizontal(|ui| {
                widgets::bullet(ui, &path.display().to_string());
                if macos::push_button(ui, "x", macos::ButtonKind::Secondary, true).clicked() {
                    remove = Some(index);
                }
            });
        }
        if let Some(index) = remove {
            paths.remove(index);
        }
    }

    widgets::hint(ui, t.t("options.drivers_hint"));
    ui.add_space(4.0);

    if app.options().drivers.is_enabled() {
        if let Some(architecture) = app.image().and_then(|image| image.architecture.as_deref()) {
            if architecture != "x86_64" {
                macos::alert(
                    ui,
                    macos::AlertKind::Warning,
                    t.t("options.drivers_x86_only"),
                );
            }
        }
    }
}

fn driver_source_label(t: &I18n, source: &DriverSource) -> String {
    match source {
        DriverSource::None => t.t("options.drivers.none").to_owned(),
        DriverSource::Auto(hardware) => t.t(hardware.key()).to_owned(),
        DriverSource::Local(paths) if paths.is_empty() => t.t("options.drivers.local").to_owned(),
        DriverSource::Local(_) => source.local_summary(),
    }
}

fn confirm(app: &mut Mac2WinApp, ui: &mut Ui) {
    let t = *app.i18n();
    widgets::section_heading(ui, t.t("step.confirm.title"));
    macos::alert(ui, macos::AlertKind::Error, t.t("step.confirm.warning"));
    ui.add_space(10.0);

    match app.build_plan() {
        Ok(plan) => {
            if plan.image.is_wtg_unsupported() {
                macos::alert(
                    ui,
                    macos::AlertKind::Warning,
                    t.t("step.confirm.wtg_unsupported"),
                );
                ui.add_space(8.0);
            }
            macos::card(ui, |ui| {
                widgets::key_value(
                    ui,
                    t.t("label.name"),
                    &format!("{} ({})", plan.device.name, plan.device.id),
                );
                widgets::key_value(ui, t.t("label.size"), &plan.device.size_human());
                widgets::key_value(ui, t.t("label.bus"), bus_label(&t, plan.device.bus));
                widgets::key_value(
                    ui,
                    t.t("step.image.selected"),
                    &plan.image.path.display().to_string(),
                );
                widgets::key_value(
                    ui,
                    t.t("label.partition_scheme"),
                    match plan.options.partition_scheme {
                        PartitionScheme::Gpt => t.t("options.gpt"),
                        PartitionScheme::Mbr => t.t("options.mbr"),
                    },
                );
                widgets::key_value(
                    ui,
                    t.t("label.format"),
                    match plan.options.file_system {
                        FileSystem::Ntfs => t.t("options.ntfs"),
                    },
                );
                if plan.options.drivers.is_enabled() {
                    let value = match &plan.options.drivers {
                        DriverSource::Auto(hardware) => t.t(hardware.key()).to_owned(),
                        DriverSource::Local(_) => plan.options.drivers.local_summary(),
                        DriverSource::None => String::new(),
                    };
                    widgets::key_value(ui, t.t("label.drivers"), &value);
                }
            });
        }
        Err(error) => macos::alert(ui, macos::AlertKind::Error, &error.to_string()),
    }

    widgets::spaced_separator(ui);
    macos::alert(ui, macos::AlertKind::Warning, t.t("app.admin_notice"));
}

fn writing(app: &mut Mac2WinApp, ui: &mut Ui) {
    let t = *app.i18n();
    widgets::section_heading(ui, t.t("step.progress.title"));

    macos::progress(ui, app.progress());
    ui.add_space(4.0);
    ui.label(
        RichText::new(format!("{:.0}%", app.progress() * 100.0))
            .weak()
            .small(),
    );

    ui.add_space(10.0);
    let current = app.stage().unwrap_or(Stage::Preparing);
    ui.label(RichText::new(t.t(current.key())).strong());
    if let Some(detail) = app.detail() {
        widgets::hint(ui, detail);
    }

    ui.add_space(12.0);
    let accent = ui.visuals().selection.bg_fill;
    let weak = ui.visuals().weak_text_color();
    for stage in Stage::ALL {
        let done = stage <= current;
        let marker = if done { "✓" } else { "•" };
        let color = if done { accent } else { weak };
        ui.label(RichText::new(format!("{marker}  {}", t.t(stage.key()))).color(color));
    }
}

fn done(app: &mut Mac2WinApp, ui: &mut Ui) {
    let t = *app.i18n();
    widgets::section_heading(ui, t.t("step.done.title"));
    macos::alert(ui, macos::AlertKind::Info, t.t("step.done.body"));
}

fn failed(app: &mut Mac2WinApp, ui: &mut Ui) {
    let t = *app.i18n();
    widgets::section_heading(ui, t.t("error.title"));
    let message = app.error().unwrap_or("unknown error").to_owned();
    macos::alert(ui, macos::AlertKind::Error, &message);
}

fn pick_image_file() -> Option<std::path::PathBuf> {
    rfd::FileDialog::new()
        .add_filter("Windows image", &["iso", "wim", "esd"])
        .add_filter("All files", &["*"])
        .pick_file()
}
