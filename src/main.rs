use mac2wingo::{AppConfig, Mac2WinApp, Services};

mod chrome;

fn main() -> eframe::Result {
    init_tracing();

    let config = AppConfig::load();
    let services = Services::system();

    let mut viewport = egui::ViewportBuilder::default()
        .with_title("Mac2Win Go")
        // Wide enough for the toolbar subtitle at the default size, and locked
        // as the minimum width so the layout can never be squeezed.
        .with_inner_size([640.0, 560.0])
        .with_min_inner_size([640.0, 560.0])
        // Draw the UI behind a transparent titlebar so the native traffic
        // lights float over our unified toolbar. An opaque titlebar would cover
        // the whole header, which fits inside the titlebar band.
        .with_fullsize_content_view(true)
        .with_titlebar_shown(false)
        .with_title_shown(false)
        .with_titlebar_buttons_shown(true);

    // Set the application icon at runtime as well as in the bundle: eframe only
    // calls `NSApplication.setApplicationIconImage` when an icon is provided, and
    // this also fixes the Dock when the binary is launched outside the `.app`.
    if let Ok(icon) = eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon_512.png")) {
        viewport = viewport.with_icon(icon);
    }

    let native_options = eframe::NativeOptions {
        viewport,
        // Avoids a macOS/AppKit crash on window close (NSTouchBar observer
        // teardown during the display cycle). With `run_and_return: false`
        // eframe uses `EventLoop::run` and exits the process directly instead
        // of tearing the window down while the run loop is still active.
        run_and_return: false,
        ..Default::default()
    };

    eframe::run_native(
        "Mac2Win Go",
        native_options,
        Box::new(move |cc: &eframe::CreationContext<'_>| {
            mac2wingo::ui::theme::apply(&cc.egui_ctx);
            let app: Box<dyn eframe::App> =
                Box::new(chrome::ChromeApp::new(Mac2WinApp::new(services, config)));
            Ok(app)
        }),
    )
}

fn init_tracing() {
    use tracing_subscriber::EnvFilter;

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let _ = tracing_subscriber::fmt().with_env_filter(filter).try_init();
}

#[cfg(test)]
mod tests {
    #[test]
    fn embedded_dock_icon_is_valid() {
        let icon = eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon_512.png"))
            .expect("embedded icon must be a valid PNG");
        assert_eq!((icon.width, icon.height), (512, 512));
        assert_eq!(icon.rgba.len(), 512 * 512 * 4);
    }
}
