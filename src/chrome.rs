//! Native window-chrome tweaks for the binary.
//!
//! This module lives outside the library so that the library can keep
//! `#![forbid(unsafe_code)]`. It centres the native traffic lights inside the
//! taller unified toolbar by moving the standard window buttons down, which
//! AppKit does not offer a safe API for.

use mac2wingo::Mac2WinApp;

/// Wraps the wizard app and re-applies the native chrome tweak every frame.
pub struct ChromeApp {
    inner: Mac2WinApp,
}

impl ChromeApp {
    pub fn new(inner: Mac2WinApp) -> Self {
        Self { inner }
    }
}

impl eframe::App for ChromeApp {
    fn logic(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.inner.logic(ctx, frame);
    }

    fn clear_color(&self, visuals: &egui::Visuals) -> [f32; 4] {
        self.inner.clear_color(visuals)
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        self.inner.ui(ui, frame);
        center_traffic_lights(ui, frame);
    }
}

/// Aligns the traffic lights with the vertical centre of the header panel.
///
/// The buttons are centred inside their titlebar container after growing that
/// container to the header height. AppKit lays out (and hit-tests) the buttons
/// within the container, so moving the buttons alone would leave the hover and
/// click regions behind. Applied idempotently: when AppKit re-lays out the
/// titlebar (resize, display change) the geometry is corrected on the next
/// repaint, and untouched frames are left alone so tracking areas can settle.
#[cfg(target_os = "macos")]
fn center_traffic_lights(ui: &egui::Ui, frame: &eframe::Frame) {
    use objc2_app_kit::{NSView, NSWindowButton};
    use raw_window_handle::{HasWindowHandle as _, RawWindowHandle};

    /// Sub-pixel slack so rounding never causes needless relayouts.
    const TOLERANCE: f64 = 0.5;

    let fullscreen = ui
        .ctx()
        .input(|input| input.viewport().fullscreen)
        .unwrap_or(false);
    if fullscreen {
        return;
    }

    let Some(window) = frame.winit_window() else {
        return;
    };
    let Ok(handle) = window.window_handle() else {
        return;
    };
    let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
        return;
    };
    let view_ptr = handle.ns_view.as_ptr().cast::<NSView>();
    if view_ptr.is_null() {
        return;
    }

    // SAFETY: `ns_view` is a valid `NSView` for as long as the window exists, and
    // we are on the main thread inside the egui repaint.
    let Some(view) = (unsafe { view_ptr.as_ref() }) else {
        return;
    };
    let Some(window) = view.window() else {
        return;
    };

    let zoom = ui.ctx().zoom_factor();
    let inset = mac2wingo::ui::theme::chrome_inset(frame, zoom);
    let panel_height = f64::from(mac2wingo::ui::theme::header_panel_height(&inset));

    let buttons: Vec<_> = [
        NSWindowButton::CloseButton,
        NSWindowButton::MiniaturizeButton,
        NSWindowButton::ZoomButton,
    ]
    .into_iter()
    .filter_map(|kind| window.standardWindowButton(kind))
    .collect();
    let Some(close) = buttons.first() else {
        return;
    };

    // The container is the titlebar view holding the buttons: its parent is the
    // window's theme frame.
    // SAFETY: the standard buttons always sit inside the titlebar container.
    let Some(titlebar) = (unsafe { close.superview() }) else {
        return;
    };
    // SAFETY: as above.
    let Some(container) = (unsafe { titlebar.superview() }) else {
        return;
    };
    // SAFETY: as above.
    let Some(parent) = (unsafe { container.superview() }) else {
        return;
    };

    let mut container_frame = container.frame();
    let top = if parent.isFlipped() {
        0.0
    } else {
        parent.bounds().size.height - panel_height
    };
    let container_moved = (container_frame.size.height - panel_height).abs() > TOLERANCE
        || (container_frame.origin.y - top).abs() > TOLERANCE;
    if container_moved {
        container_frame.size.height = panel_height;
        container_frame.origin.y = top;
        container.setFrame(container_frame);
        container.updateTrackingAreas();
    }

    for button in &buttons {
        let mut button_frame = button.frame();
        let centred = (panel_height - button_frame.size.height) / 2.0;
        if (button_frame.origin.y - centred).abs() > TOLERANCE {
            button_frame.origin.y = centred;
            button.setFrame(button_frame);
            button.updateTrackingAreas();
        }
    }
}

#[cfg(not(target_os = "macos"))]
fn center_traffic_lights(_ui: &egui::Ui, _frame: &eframe::Frame) {}
