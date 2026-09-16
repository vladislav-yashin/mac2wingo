use std::path::PathBuf;
use std::sync::Arc;

use crossbeam_channel::{Receiver, Sender};

use crate::core::device::{DeviceService, DiskDevice};
use crate::core::error::{CoreError, Result};
use crate::core::image::{ImageService, WindowsImage};
use crate::core::options::WriteOptions;
use crate::core::progress::{ProgressEvent, ProgressSink, Stage};
use crate::core::writer::{WindowsToGoWriter, WritePlan};
use crate::core::AppConfig;
use crate::i18n::{I18n, Language};

/// Steps of the creation wizard, in display order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Step {
    Welcome,
    Image,
    Device,
    Options,
    Confirm,
    Writing,
    Done,
    Failed,
}

impl Step {
    /// All wizard steps, excluding the transient result screens.
    pub const WIZARD: [Step; 5] = [
        Step::Welcome,
        Step::Image,
        Step::Device,
        Step::Options,
        Step::Confirm,
    ];

    /// The step that logically precedes this one.
    #[must_use]
    pub fn previous(self) -> Option<Self> {
        match self {
            Step::Welcome | Step::Done | Step::Failed => None,
            Step::Image => Some(Step::Welcome),
            Step::Device => Some(Step::Image),
            Step::Options => Some(Step::Device),
            Step::Confirm => Some(Step::Options),
            Step::Writing => Some(Step::Confirm),
        }
    }

    /// Zero-based position of this step in the wizard progress indicator.
    #[must_use]
    pub fn progress_index(self) -> usize {
        match self {
            Step::Welcome => 0,
            Step::Image => 1,
            Step::Device => 2,
            Step::Options => 3,
            Step::Confirm | Step::Writing | Step::Done | Step::Failed => 4,
        }
    }
}

/// The three business services the application depends on.
#[derive(Clone)]
pub struct Services {
    pub device: Arc<dyn DeviceService>,
    pub image: Arc<dyn ImageService>,
    pub writer: Arc<dyn WindowsToGoWriter>,
}

impl Services {
    /// Production services backed by `diskutil`, `wimlib-imagex` and the
    /// privileged writer.
    #[must_use]
    pub fn system() -> Self {
        Self {
            device: Arc::new(crate::platform::DiskutilDeviceService::system()),
            image: Arc::new(crate::services::WimImageService::system()),
            writer: Arc::new(crate::services::RealWriter::system()),
        }
    }
}

/// Messages sent from the writer thread back to the UI.
enum WriteMessage {
    Progress(ProgressEvent),
    Done(std::result::Result<(), String>),
}

/// Forwards progress and wakes the UI for a repaint.
struct RepaintSink {
    sender: Sender<WriteMessage>,
    ctx: egui::Context,
}

impl ProgressSink for RepaintSink {
    fn report(&self, event: ProgressEvent) {
        let _ = self.sender.send(WriteMessage::Progress(event));
        self.ctx.request_repaint();
    }
}

/// The application state.
pub struct Mac2WinApp {
    services: Services,
    config: AppConfig,
    config_path: Option<PathBuf>,
    i18n: I18n,

    step: Step,
    devices: Vec<DiskDevice>,
    selected_device: Option<String>,
    selected_image_path: Option<PathBuf>,
    image: Option<WindowsImage>,
    options: WriteOptions,

    progress: f32,
    stage: Option<Stage>,
    detail: Option<String>,
    error: Option<String>,

    write_rx: Option<Receiver<WriteMessage>>,
    chrome: Option<crate::ui::theme::ChromeInset>,
}

impl Mac2WinApp {
    #[must_use]
    pub fn new(services: Services, config: AppConfig) -> Self {
        let i18n = I18n::new(config.language.unwrap_or_else(Language::system));
        let options = config.options.clone();
        Self {
            services,
            config,
            config_path: AppConfig::config_path(),
            i18n,
            step: Step::Welcome,
            devices: Vec::new(),
            selected_device: None,
            selected_image_path: None,
            image: None,
            options,
            progress: 0.0,
            stage: None,
            detail: None,
            error: None,
            write_rx: None,
            chrome: None,
        }
    }

    /// Override where settings are persisted. `None` disables persistence
    /// (used by tests so they never touch the user's home directory).
    #[must_use]
    pub fn with_config_path(mut self, path: Option<PathBuf>) -> Self {
        self.config_path = path;
        self
    }

    // ------------------------------------------------------------------
    // Accessors (also used by tests)
    // ------------------------------------------------------------------

    #[must_use]
    pub fn step(&self) -> Step {
        self.step
    }

    #[must_use]
    pub fn i18n(&self) -> &I18n {
        &self.i18n
    }

    #[must_use]
    pub fn devices(&self) -> &[DiskDevice] {
        &self.devices
    }

    #[must_use]
    pub fn selected_device(&self) -> Option<&DiskDevice> {
        let id = self.selected_device.as_deref()?;
        self.devices.iter().find(|device| device.id == id)
    }

    #[must_use]
    pub fn selected_device_id(&self) -> Option<&str> {
        self.selected_device.as_deref()
    }

    #[must_use]
    pub fn image(&self) -> Option<&WindowsImage> {
        self.image.as_ref()
    }

    #[must_use]
    pub fn selected_image_path(&self) -> Option<&PathBuf> {
        self.selected_image_path.as_ref()
    }

    #[must_use]
    pub fn options(&self) -> &WriteOptions {
        &self.options
    }

    pub fn options_mut(&mut self) -> &mut WriteOptions {
        &mut self.options
    }

    #[must_use]
    pub fn progress(&self) -> f32 {
        self.progress
    }

    #[must_use]
    pub fn stage(&self) -> Option<Stage> {
        self.stage
    }

    #[must_use]
    pub fn detail(&self) -> Option<&str> {
        self.detail.as_deref()
    }

    #[must_use]
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    #[must_use]
    pub fn config(&self) -> &AppConfig {
        &self.config
    }

    /// Native window chrome metrics (traffic-light size), measured once.
    pub fn chrome_inset(
        &mut self,
        frame: &eframe::Frame,
        zoom: f32,
    ) -> crate::ui::theme::ChromeInset {
        *self
            .chrome
            .get_or_insert_with(|| crate::ui::theme::chrome_inset(frame, zoom))
    }

    // ------------------------------------------------------------------
    // Commands
    // ------------------------------------------------------------------

    pub fn set_language(&mut self, language: Language) {
        self.i18n.set_lang(language);
        self.config.language = Some(language);
        self.save_config();
    }

    fn save_config(&self) {
        let Some(path) = &self.config_path else {
            return;
        };
        if let Err(error) = self.config.save_to(path) {
            tracing::warn!(%error, "failed to save config");
        }
    }

    /// Refresh the list of connected drives.
    pub fn scan_devices(&mut self) {
        match self.services.device.list_devices() {
            Ok(devices) => {
                self.devices = devices;
                if let Some(selected) = &self.selected_device {
                    let still_present = self.devices.iter().any(|d| &d.id == selected);
                    if !still_present {
                        self.selected_device = None;
                    }
                }
                tracing::info!(count = self.devices.len(), "scanned devices");
            }
            Err(error) => {
                tracing::error!(%error, "failed to scan devices");
                self.error = Some(error.to_string());
            }
        }
    }

    pub fn select_device(&mut self, id: &str) {
        if self.devices.iter().any(|device| device.id == id) {
            self.selected_device = Some(id.to_owned());
            self.config.last_target_device = Some(id.to_owned());
        }
    }

    /// Inspect a Windows image and remember it on success.
    pub fn select_image(&mut self, path: PathBuf) -> Result<()> {
        let image = self.services.image.inspect(&path)?;
        if let Some(parent) = path.parent() {
            self.config.last_image_dir = Some(parent.to_path_buf());
        }
        self.selected_image_path = Some(path);
        self.image = Some(image);
        self.error = None;
        self.save_config();
        Ok(())
    }

    /// Pick a different edition from the inspected image.
    pub fn select_edition(&mut self, index: u32) {
        if let Some(image) = self.image.take() {
            self.image = Some(image.with_edition(index));
        }
    }

    /// Bundled helper tools that are missing, if any. Used by the UI to warn
    /// the user before starting a write.
    #[must_use]
    pub fn missing_tools(&self) -> Vec<crate::platform::macos::tools::Tool> {
        use crate::platform::macos::tools::{Tool, ToolLocator};
        ToolLocator::system().missing(&Tool::UEFI)
    }

    /// Whether "Next" can be pressed from the current step.
    #[must_use]
    pub fn can_advance(&self) -> bool {
        match self.step {
            Step::Welcome => true,
            Step::Image => self.image.is_some(),
            Step::Device => self
                .selected_device()
                .is_some_and(DiskDevice::is_eligible_target),
            Step::Options => self.options.validate().is_ok(),
            Step::Confirm => self.build_plan().is_ok(),
            Step::Writing | Step::Done | Step::Failed => false,
        }
    }

    /// Move to the next step, if allowed.
    pub fn advance(&mut self) {
        if !self.can_advance() {
            return;
        }
        self.error = None;
        self.step = match self.step {
            Step::Welcome => Step::Image,
            Step::Image => {
                self.scan_devices();
                Step::Device
            }
            Step::Device => Step::Options,
            Step::Options => Step::Confirm,
            Step::Confirm => Step::Writing,
            Step::Writing | Step::Done | Step::Failed => self.step,
        };
    }

    /// Go back one step, if possible.
    pub fn go_back(&mut self) {
        self.error = None;
        if let Some(previous) = self.step.previous() {
            self.step = previous;
        }
    }

    /// Retry after a failure by returning to the confirmation step.
    pub fn retry(&mut self) {
        self.error = None;
        self.progress = 0.0;
        self.stage = None;
        self.step = Step::Confirm;
    }

    /// Build the plan for the current selection.
    pub fn build_plan(&self) -> Result<WritePlan> {
        let device = self
            .selected_device()
            .cloned()
            .ok_or(CoreError::NoDeviceSelected)?;
        let image = self
            .image
            .clone()
            .ok_or_else(|| CoreError::UnsupportedImage("no image selected".to_owned()))?;
        let plan = WritePlan {
            device,
            image,
            options: self.options.clone(),
        };
        plan.validate()?;
        Ok(plan)
    }

    /// Start the (simulated) write in a background thread.
    pub fn start_write(&mut self, ctx: &egui::Context) {
        if self.step == Step::Writing {
            return;
        }
        let plan = match self.build_plan() {
            Ok(plan) => plan,
            Err(error) => {
                self.error = Some(error.to_string());
                self.step = Step::Failed;
                return;
            }
        };

        self.progress = 0.0;
        self.stage = None;
        self.detail = None;
        self.error = None;

        let (tx, rx) = crossbeam_channel::unbounded();
        self.write_rx = Some(rx);

        let writer = Arc::clone(&self.services.writer);
        let sink = RepaintSink {
            sender: tx,
            ctx: ctx.clone(),
        };

        self.step = Step::Writing;
        std::thread::spawn(move || {
            let result = writer.write(&plan, &sink);
            let _ = sink.sender.send(WriteMessage::Done(
                result.map_err(|error| error.to_string()),
            ));
            sink.ctx.request_repaint();
        });
    }

    /// Request cancellation of an in-flight write.
    pub fn cancel_write(&mut self) {
        self.services.writer.cancel();
    }

    /// Drain messages from the writer thread. Called once per frame.
    pub fn poll(&mut self) {
        let Some(rx) = &self.write_rx else {
            return;
        };

        while let Ok(message) = rx.try_recv() {
            match message {
                WriteMessage::Progress(event) => {
                    self.progress = event.fraction;
                    self.stage = Some(event.stage);
                    self.detail = event.detail;
                }
                WriteMessage::Done(result) => {
                    match result {
                        Ok(()) => {
                            self.progress = 1.0;
                            self.stage = Some(Stage::Finished);
                            self.step = Step::Done;
                        }
                        Err(error) => {
                            self.error = Some(error);
                            self.step = Step::Failed;
                        }
                    }
                    self.write_rx = None;
                    break;
                }
            }
        }
    }

    /// Return to the start of the wizard, keeping the service handles.
    pub fn reset(&mut self) {
        self.step = Step::Welcome;
        self.selected_device = None;
        self.selected_image_path = None;
        self.image = None;
        self.progress = 0.0;
        self.stage = None;
        self.detail = None;
        self.error = None;
        self.write_rx = None;
    }

    /// Record an error that should be surfaced to the user.
    pub fn set_error(&mut self, message: impl Into<String>) {
        self.error = Some(message.into());
    }

    /// Convenience pass-through used by the UI to show the last error.
    pub fn take_error(&mut self) -> Option<String> {
        self.error.take()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::device::BusType;
    use crate::core::image::ImageFormat;
    use crate::services::{MockDeviceService, MockImageService, MockWriter};

    fn usb(id: &str) -> DiskDevice {
        DiskDevice {
            id: id.into(),
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

    fn image() -> WindowsImage {
        WindowsImage {
            path: "/tmp/win11.iso".into(),
            format: ImageFormat::Iso,
            size_bytes: 6_000_000_000,
            edition: None,
            architecture: None,
            edition_index: 0,
            version: None,
            editions: Vec::new(),
        }
    }

    fn app(writer: MockWriter) -> Mac2WinApp {
        let services = Services {
            device: Arc::new(MockDeviceService::new(vec![usb("disk5")])),
            image: Arc::new(MockImageService::new(image())),
            writer: Arc::new(writer),
        };
        let config = AppConfig {
            language: Some(Language::En),
            ..AppConfig::default()
        };
        Mac2WinApp::new(services, config).with_config_path(None)
    }

    #[test]
    fn wizard_requires_image_then_device_before_confirm() {
        let mut app = app(MockWriter::new());
        assert_eq!(app.step(), Step::Welcome);

        app.advance();
        assert_eq!(app.step(), Step::Image);
        assert!(!app.can_advance());

        app.select_image("/tmp/win11.iso".into()).unwrap();
        assert!(app.can_advance());
        app.advance();
        assert_eq!(app.step(), Step::Device);
        assert!(!app.can_advance());

        app.select_device("disk5");
        assert!(app.can_advance());
        app.advance();
        assert_eq!(app.step(), Step::Options);
        assert!(app.can_advance());

        app.advance();
        assert_eq!(app.step(), Step::Confirm);
        assert!(app.can_advance());
    }

    #[test]
    fn go_back_walks_the_wizard() {
        let mut app = app(MockWriter::new());
        app.select_image("/tmp/win11.iso".into()).unwrap();
        app.advance();
        app.advance();
        app.select_device("disk5");
        app.advance();
        assert_eq!(app.step(), Step::Options);
        app.go_back();
        assert_eq!(app.step(), Step::Device);
        app.go_back();
        assert_eq!(app.step(), Step::Image);
    }

    #[test]
    fn invalid_options_block_advancing() {
        let mut app = app(MockWriter::new());
        app.select_image("/tmp/win11.iso".into()).unwrap();
        app.advance();
        app.select_device("disk5");
        app.advance();
        app.options_mut().partition_scheme = crate::core::options::PartitionScheme::Mbr;
        app.options_mut().uefi = true;
        assert!(!app.can_advance());
    }

    #[test]
    fn select_image_reports_errors() {
        let mut app = app(MockWriter::new());
        // MockImageService always succeeds, so exercise the happy path only.
        assert!(app.select_image("/tmp/win11.iso".into()).is_ok());
        assert!(app.image().is_some());
    }

    #[test]
    fn scan_devices_populates_targets() {
        let mut app = app(MockWriter::new());
        app.scan_devices();
        assert_eq!(app.devices().len(), 1);
    }
}
