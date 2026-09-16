use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use crate::core::error::{CoreError, Result};
use crate::core::progress::{ProgressEvent, ProgressSink, Stage};
use crate::core::writer::{WindowsToGoWriter, WritePlan};

/// Number of progress ticks simulated per stage.
const TICKS_PER_STAGE: u32 = 10;

/// Simulated writer used during stage 1.
///
/// It performs **no** destructive operations. It validates the plan, walks the
/// [`Stage`] list and emits progress events with a configurable delay so the UI
/// can be exercised end to end.
///
/// TODO(stage-2): replace with the privileged implementation that drives
/// `diskutil`, `mkfs.ntfs`/`ntfs-3g`, `wimlib-imagex` and `bcdboot`.
pub struct StubWriter {
    cancelled: AtomicBool,
    step_delay: Duration,
}

impl Default for StubWriter {
    fn default() -> Self {
        Self::new()
    }
}

impl StubWriter {
    /// Default simulation pace (a short pause per tick).
    #[must_use]
    pub fn new() -> Self {
        Self {
            cancelled: AtomicBool::new(false),
            step_delay: Duration::from_millis(80),
        }
    }

    /// Set the delay between progress ticks. Pass [`Duration::ZERO`] in tests.
    #[must_use]
    pub fn with_step_delay(mut self, step_delay: Duration) -> Self {
        self.step_delay = step_delay;
        self
    }

    fn sleep(&self) {
        if !self.step_delay.is_zero() {
            std::thread::sleep(self.step_delay);
        }
    }
}

impl WindowsToGoWriter for StubWriter {
    fn write(&self, plan: &WritePlan, sink: &dyn ProgressSink) -> Result<()> {
        plan.validate()?;

        let total_ticks = (Stage::ALL.len() as u32 * TICKS_PER_STAGE) as f32;

        for (stage_index, stage) in Stage::ALL.iter().enumerate() {
            for tick in 1..=TICKS_PER_STAGE {
                if self.cancelled.load(Ordering::SeqCst) {
                    return Err(CoreError::Cancelled);
                }

                let done = (stage_index as u32 * TICKS_PER_STAGE + tick) as f32;
                let detail = match stage {
                    Stage::ApplyingImage => {
                        format!(
                            "{} — {:.0}%",
                            plan.image.path.display(),
                            done / total_ticks * 100.0
                        )
                    }
                    _ => format!("{} {tick}/{TICKS_PER_STAGE}", stage.key()),
                };

                sink.report(ProgressEvent::new(*stage, done / total_ticks).with_detail(detail));
                self.sleep();
            }
        }

        sink.report(ProgressEvent::new(Stage::Finished, 1.0));
        Ok(())
    }

    fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::device::{BusType, DiskDevice};
    use crate::core::image::{ImageFormat, WindowsImage};
    use crate::core::options::WriteOptions;
    use std::sync::Mutex;

    fn plan() -> WritePlan {
        WritePlan {
            device: DiskDevice {
                id: "disk5".into(),
                name: "SanDisk".into(),
                size_bytes: 32_000_000_000,
                block_size: 512,
                bus: BusType::Usb,
                is_removable: true,
                is_internal: false,
                partition_scheme: None,
                volumes: vec![],
            },
            image: WindowsImage {
                path: "/tmp/win11.iso".into(),
                format: ImageFormat::Iso,
                size_bytes: 6_000_000_000,
                edition: None,
                architecture: None,
                edition_index: 0,
                version: None,
                editions: Vec::new(),
            },
            options: WriteOptions::default(),
        }
    }

    #[derive(Default)]
    struct RecordingSink(Mutex<Vec<ProgressEvent>>);

    impl ProgressSink for RecordingSink {
        fn report(&self, event: ProgressEvent) {
            self.0.lock().unwrap().push(event);
        }
    }

    #[test]
    fn emits_events_for_every_stage_and_finishes() {
        let writer = StubWriter::new().with_step_delay(Duration::ZERO);
        let sink = RecordingSink::default();

        writer.write(&plan(), &sink).unwrap();

        let events = sink.0.lock().unwrap();
        assert!(!events.is_empty());
        assert_eq!(events.last().unwrap().stage, Stage::Finished);
        assert!((events.last().unwrap().fraction - 1.0).abs() < f32::EPSILON);

        for stage in Stage::ALL {
            assert!(
                events.iter().any(|event| event.stage == stage),
                "missing progress for {stage:?}"
            );
        }
    }

    #[test]
    fn progress_fraction_is_monotonic() {
        let writer = StubWriter::new().with_step_delay(Duration::ZERO);
        let sink = RecordingSink::default();
        writer.write(&plan(), &sink).unwrap();

        let events = sink.0.lock().unwrap();
        for pair in events.windows(2) {
            assert!(pair[1].fraction >= pair[0].fraction);
        }
    }

    #[test]
    fn rejects_invalid_plan() {
        let writer = StubWriter::new().with_step_delay(Duration::ZERO);
        let mut invalid = plan();
        invalid.device.bus = BusType::DiskImage;
        assert!(matches!(
            writer.write(&invalid, &crate::core::progress::NullProgressSink),
            Err(CoreError::NotEligible(_))
        ));
    }

    #[test]
    fn cancellation_stops_the_write() {
        let writer = StubWriter::new().with_step_delay(Duration::from_millis(1));
        writer.cancel();
        let result = writer.write(&plan(), &crate::core::progress::NullProgressSink);
        assert!(matches!(result, Err(CoreError::Cancelled)));
    }
}
