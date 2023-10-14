// error_mod.rs

//! I am using the crate thiserror to create an enum for all library errors.
//! It mostly forwards the source "from" error.
//! The library never writes to the screen, because it contains only the logic.
//! Is the bin project that knows if it is CLI, TUI or GUI and it presents the errors to the user and developer.
//! Then in the bin project I use the crate anyhow.

/// list of possible errors from this library
#[derive(thiserror::Error, Debug)]
pub enum LibError {
    #[error("VarError: {0}")]
    VarError(#[from] std::env::VarError),

    #[error("IoError: {0}")]
    IoError(#[from] std::io::Error),

    #[error("SerdeJsonError: {0}")]
    SerdeJsonError(#[from] serde_json::Error),

    #[error("DecryptionError: {0}")]
    DecryptionError(#[from] fernet::DecryptionError),

    #[error("FromUtf8Error: {0}")]
    FromUtf8Error(#[from] std::string::FromUtf8Error),

    #[error("DropboxError: {0}")]
    DropboxError(#[from] dropbox_sdk::Error),
    #[error("ListFolderError: {0}")]
    ListFolderError(#[from] dropbox_sdk::files::ListFolderError),

    #[error("InquireError: {0}")]
    InquireError(#[from] inquire::InquireError),

    #[error("ErrorFromString: {0}")]
    ErrorFromString(String),
    #[error("ErrorFromStaticStr: {0}")]
    ErrorFromStr(&'static str),
    #[error("unknown error")]
    UnknownError,
}
