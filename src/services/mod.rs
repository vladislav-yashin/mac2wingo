//! Concrete and fake service implementations.

pub mod mock;
pub mod real_image;
pub mod real_writer;
pub mod stub_image;
pub mod stub_writer;

use crossbeam_channel::Sender;

use crate::core::progress::{ProgressEvent, ProgressSink};

pub use mock::{MockDeviceService, MockImageService, MockWriter};
pub use real_image::WimImageService;
pub use real_writer::RealWriter;
pub use stub_image::StubImageService;
pub use stub_writer::StubWriter;

/// A [`ProgressSink`] that forwards events to a channel.
#[derive(Debug, Clone)]
pub struct ChannelProgressSink {
    sender: Sender<ProgressEvent>,
}

impl ChannelProgressSink {
    #[must_use]
    pub fn new(sender: Sender<ProgressEvent>) -> Self {
        Self { sender }
    }
}

impl ProgressSink for ChannelProgressSink {
    fn report(&self, event: ProgressEvent) {
        // A closed channel simply means the UI went away; dropping is fine.
        let _ = self.sender.send(event);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::progress::Stage;

    #[test]
    fn forwards_events_to_channel() {
        let (tx, rx) = crossbeam_channel::unbounded();
        let sink = ChannelProgressSink::new(tx);
        sink.report(ProgressEvent::new(Stage::Preparing, 0.5));
        assert_eq!(rx.recv().unwrap().stage, Stage::Preparing);
    }
}
