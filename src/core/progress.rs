/// Sequential stages of writing a Windows To Go drive.
///
/// The order is meaningful: the UI uses it to render a step list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Stage {
    PreparingDrivers,
    Preparing,
    Partitioning,
    Formatting,
    ApplyingImage,
    InstallingDrivers,
    Configuring,
    InstallingBootloader,
    Verifying,
    Ejecting,
    Finished,
}

impl Stage {
    /// All stages in execution order.
    pub const ALL: [Stage; 11] = [
        Stage::PreparingDrivers,
        Stage::Preparing,
        Stage::Partitioning,
        Stage::Formatting,
        Stage::ApplyingImage,
        Stage::InstallingDrivers,
        Stage::Configuring,
        Stage::InstallingBootloader,
        Stage::Verifying,
        Stage::Ejecting,
        Stage::Finished,
    ];

    /// Stable i18n key for this stage.
    #[must_use]
    pub fn key(self) -> &'static str {
        match self {
            Stage::PreparingDrivers => "stage.preparing_drivers",
            Stage::Preparing => "stage.preparing",
            Stage::Partitioning => "stage.partitioning",
            Stage::Formatting => "stage.formatting",
            Stage::ApplyingImage => "stage.applying_image",
            Stage::InstallingDrivers => "stage.installing_drivers",
            Stage::Configuring => "stage.configuring",
            Stage::InstallingBootloader => "stage.installing_bootloader",
            Stage::Verifying => "stage.verifying",
            Stage::Ejecting => "stage.ejecting",
            Stage::Finished => "stage.finished",
        }
    }

    /// Machine readable token used in privileged script markers.
    #[must_use]
    pub fn token(self) -> &'static str {
        match self {
            Stage::PreparingDrivers => "preparing_drivers",
            Stage::Preparing => "preparing",
            Stage::Partitioning => "partitioning",
            Stage::Formatting => "formatting",
            Stage::ApplyingImage => "applying_image",
            Stage::InstallingDrivers => "installing_drivers",
            Stage::Configuring => "configuring",
            Stage::InstallingBootloader => "installing_bootloader",
            Stage::Verifying => "verifying",
            Stage::Ejecting => "ejecting",
            Stage::Finished => "finished",
        }
    }

    /// Parse a token produced by [`Stage::token`].
    #[must_use]
    pub fn from_token(token: &str) -> Option<Self> {
        Self::ALL
            .iter()
            .copied()
            .find(|stage| stage.token() == token)
    }

    /// Fraction of the overall operation that starts this stage.
    #[must_use]
    pub fn start_fraction(self) -> f32 {
        let index = Self::ALL
            .iter()
            .position(|stage| *stage == self)
            .unwrap_or(0);
        index as f32 / Self::ALL.len() as f32
    }
}

/// A progress update emitted by a writer implementation.
#[derive(Debug, Clone, PartialEq)]
pub struct ProgressEvent {
    pub stage: Stage,
    /// Overall completion in the `0.0..=1.0` range.
    pub fraction: f32,
    /// Optional human readable detail (file name, current step, ...).
    pub detail: Option<String>,
}

impl ProgressEvent {
    #[must_use]
    pub fn new(stage: Stage, fraction: f32) -> Self {
        Self {
            stage,
            fraction: fraction.clamp(0.0, 1.0),
            detail: None,
        }
    }

    #[must_use]
    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }
}

/// Receives progress updates from a background writer.
pub trait ProgressSink: Send + Sync {
    fn report(&self, event: ProgressEvent);
}

/// Discards all events; useful in tests and no-op contexts.
#[derive(Debug, Default, Clone, Copy)]
pub struct NullProgressSink;

impl ProgressSink for NullProgressSink {
    fn report(&self, _event: ProgressEvent) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stages_are_ordered() {
        assert!(Stage::Preparing < Stage::Partitioning);
        assert!(Stage::ApplyingImage < Stage::InstallingDrivers);
        assert!(Stage::InstallingDrivers < Stage::Configuring);
        assert!(Stage::Configuring < Stage::InstallingBootloader);
        assert!(Stage::ApplyingImage < Stage::Finished);
        assert_eq!(Stage::ALL.len(), 11);
    }

    #[test]
    fn fraction_is_clamped() {
        assert!((ProgressEvent::new(Stage::Preparing, 2.0).fraction - 1.0).abs() < f32::EPSILON);
        assert!((ProgressEvent::new(Stage::Preparing, -1.0).fraction - 0.0).abs() < f32::EPSILON);
    }
}
