use thiserror::Error;

pub type Result<T> = std::result::Result<T, AudioFocusError>;

#[derive(Debug, Error)]
pub enum AudioFocusError {
    #[error("Windows API call failed: {0}")]
    Windows(#[from] windows::core::Error),

    #[error("failed to install shutdown handler: {0}")]
    CtrlC(#[from] ctrlc::Error),

    #[error("logging initialization failed: {0}")]
    Logging(String),

    #[error("thread error: {0}")]
    Thread(String),

    #[error("SMTC error: {0}")]
    Smtc(String),

    #[error("non-SMTC controller error: {0}")]
    NonSmtc(String),

    #[error("Win32 error: {0}")]
    Win32(String),
}
