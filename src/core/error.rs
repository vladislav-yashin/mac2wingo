use thiserror::Error;

/// Errors produced by the platform-independent core layer.
#[derive(Debug, Error)]
pub enum CoreError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("failed to parse disk information: {0}")]
    Parse(String),

    #[error("no target drive selected")]
    NoDeviceSelected,

    #[error("drive `{0}` was not found")]
    DeviceNotFound(String),

    #[error("drive `{0}` is not an eligible external drive")]
    NotEligible(String),

    #[error("unsupported image format: {0}")]
    UnsupportedImage(String),

    #[error("image file does not exist: {0}")]
    ImageNotFound(String),

    #[error("invalid configuration: {0}")]
    InvalidConfig(String),

    #[error("operation cancelled")]
    Cancelled,

    #[error("required tool `{0}` was not found; expected it to be bundled with the app")]
    ToolMissing(String),

    #[error("failed to obtain administrator privileges: {0}")]
    Privileged(String),

    #[error("administrator command failed: {0}")]
    CommandFailed(String),

    #[error("failed to partition `{0}`")]
    Partition(String),

    #[error("failed to mount `{0}`")]
    Mount(String),

    #[error("failed to apply the Windows image: {0}")]
    ImageApply(String),

    #[error("failed to install the bootloader: {0}")]
    Bootloader(String),

    #[error("operation failed: {0}")]
    Operation(String),
}

pub type Result<T> = std::result::Result<T, CoreError>;
