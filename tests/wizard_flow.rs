//! Headless UI tests driving the wizard through `egui_kittest`.

use std::sync::Arc;

use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;

use mac2wingo::app::Services;
use mac2wingo::core::device::{BusType, DiskDevice};
use mac2wingo::core::image::{ImageFormat, WindowsImage};
use mac2wingo::core::AppConfig;
use mac2wingo::i18n::Language;
use mac2wingo::services::{MockDeviceService, MockImageService, MockWriter};
use mac2wingo::{Mac2WinApp, Step};

fn usb_drive() -> DiskDevice {
    DiskDevice {
        id: "disk5".into(),
        name: "SanDisk Ultra".into(),
        size_bytes: 32_000_000_000,
        block_size: 512,
        bus: BusType::Usb,
        is_removable: true,
        is_internal: false,
        partition_scheme: Some("GUID_partition_scheme".into()),
        volumes: vec![],
    }
}

fn app(writer: MockWriter) -> Mac2WinApp {
    let image = WindowsImage {
        path: "/mock/win11.iso".into(),
        format: ImageFormat::Iso,
        size_bytes: 6_000_000_000,
        edition: None,
        architecture: None,
        edition_index: 0,
        version: None,
        editions: Vec::new(),
    };
    let services = Services {
        device: Arc::new(MockDeviceService::new(vec![usb_drive()])),
        image: Arc::new(MockImageService::new(image)),
        writer: Arc::new(writer),
    };
    let config = AppConfig {
        language: Some(Language::En),
        ..AppConfig::default()
    };
    Mac2WinApp::new(services, config).with_config_path(None)
}

fn harness(writer: MockWriter) -> Harness<'static, Mac2WinApp> {
    Harness::builder()
        .with_size(egui::Vec2::new(960.0, 720.0))
        .build_eframe(move |_cc| app(writer))
}

fn run_until(harness: &mut Harness<'_, Mac2WinApp>, target: Step) {
    for _ in 0..100 {
        harness.run();
        if harness.state().step() == target {
            return;
        }
    }
    panic!(
        "wizard stuck at {:?}, expected {target:?}",
        harness.state().step()
    );
}

#[test]
fn walks_the_whole_wizard_to_done() {
    let mut harness = harness(MockWriter::new());

    // Header is rendered.
    assert!(harness.query_by_label("Mac2Win Go").is_some());

    harness.get_by_label("Next").click();
    run_until(&mut harness, Step::Image);

    harness
        .state_mut()
        .select_image("/mock/win11.iso".into())
        .unwrap();
    harness.get_by_label("Next").click();
    run_until(&mut harness, Step::Device);

    harness.get_by_label_contains("SanDisk").click();
    run_until(&mut harness, Step::Device);
    assert_eq!(harness.state().selected_device_id(), Some("disk5"));

    harness.get_by_label("Next").click();
    run_until(&mut harness, Step::Options);

    harness.get_by_label("Next").click();
    run_until(&mut harness, Step::Confirm);

    harness.get_by_label("Start").click();
    run_until(&mut harness, Step::Done);

    assert_eq!(harness.state().progress(), 1.0);
}

#[test]
fn header_content_is_vertically_centred() {
    use mac2wingo::ui::theme::{header_panel_height, ChromeInset};

    let mut harness = harness(MockWriter::new());
    harness.run();

    // The header is a unified toolbar: the row is centred in the panel, as are
    // the native traffic lights.
    let expected_center = header_panel_height(&ChromeInset::DEFAULT) / 2.0;

    let title = harness.query_by_label("Mac2Win Go").unwrap().rect();
    let combo = harness.query_by_value("English").unwrap().rect();

    assert!(
        (title.center().y - combo.center().y).abs() <= 0.5,
        "title and language selector are on different rows: {} vs {}",
        title.center().y,
        combo.center().y
    );

    for (name, rect) in [("title", title), ("language selector", combo)] {
        assert!(
            (rect.center().y - expected_center).abs() <= 2.0,
            "{name} is not vertically centred: centre {} vs expected {expected_center}",
            rect.center().y
        );
    }
}

#[test]
fn surfaces_image_selection_errors() {
    let mut harness = Harness::builder()
        .with_size(egui::Vec2::new(960.0, 720.0))
        .build_eframe(move |_cc| {
            let services = Services {
                device: Arc::new(MockDeviceService::new(vec![usb_drive()])),
                image: Arc::new(MockImageService::failing()),
                writer: Arc::new(MockWriter::new()),
            };
            let config = AppConfig {
                language: Some(Language::En),
                ..AppConfig::default()
            };
            Mac2WinApp::new(services, config).with_config_path(None)
        });

    harness.get_by_label("Next").click();
    run_until(&mut harness, Step::Image);

    let error = harness
        .state_mut()
        .select_image("/tmp/broken.iso".into())
        .unwrap_err();
    harness.state_mut().set_error(error.to_string());

    harness.run();
    assert!(harness.query_by_label_contains("broken.iso").is_some());
}

#[test]
fn surfaces_write_failures() {
    let mut harness = harness(MockWriter::failing());

    harness.get_by_label("Next").click();
    run_until(&mut harness, Step::Image);
    harness
        .state_mut()
        .select_image("/mock/win11.iso".into())
        .unwrap();
    harness.get_by_label("Next").click();
    run_until(&mut harness, Step::Device);
    harness.get_by_label_contains("SanDisk").click();
    harness.run();
    harness.get_by_label("Next").click();
    run_until(&mut harness, Step::Options);
    harness.get_by_label("Next").click();
    run_until(&mut harness, Step::Confirm);
    harness.get_by_label("Start").click();
    run_until(&mut harness, Step::Failed);

    assert!(harness.state().error().is_some());
}
