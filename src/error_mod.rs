// error_mod.rs

//! I am using the crate thiserror to create an enum for all library errors.
//! It mostly forwards the source "from" error.
//! The library never writes to the screen, because it contains only the logic.
//! Is the bin project that knows if it is CLI, TUI or GUI and it presents the errors to the user and developer.
//! Then in the bin project I use the crate anyhow.

/// dropbox_backup_to_external_disk_lib::Error
///
/// It is a list of all possible errors from this library.
#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("VarError: {0}")]
    VarError(#[from] std::env::VarError),

    #[error("IoError: {0}")]
    IoError(#[from] std::io::Error),

    #[error("WalkdirError: {0}")]
    WalkdirError(#[from] walkdir::Error),

    #[error("SerdeJsonError: {0}")]
    SerdeJsonError(#[from] serde_json::Error),

    #[error("FromUtf8Error: {0}")]
    FromUtf8Error(#[from] std::string::FromUtf8Error),

    #[error("DropboxError: {0}")]
    DropboxError(#[from] dropbox_sdk::Error),
    #[error("ListFolderError: {0}")]
    ListFolderError(#[from] dropbox_sdk::files::ListFolderError),

    #[error("GetMetadataError: {0}")]
    GetMetadataError(#[from] dropbox_sdk::files::GetMetadataError),

    #[error("InquireError: {0}")]
    InquireError(#[from] inquire::InquireError),

    #[error("TimestampError: {0}")]
    TimestampError(#[from] humantime::TimestampError),

    #[error("CrossPlatformPathError: {0}")]
    CrossPlatformPathError(#[from] crossplatform_path::Error),

    #[error(transparent)]
    ChronoParseError(#[from] chrono::ParseError),

    #[error(transparent)]
    ReqwestError(#[from] reqwest::Error),

    #[error(transparent)]
    SshKeyError(#[from] ssh_key::Error),

    #[error(transparent)]
    RsaSignatureError(#[from] rsa::signature::Error),

    #[error(transparent)]
    Base64Error(#[from] base64ct::Error),

    #[error("ErrorFromString: {0}")]
    ErrorFromString(String),
    #[error("ErrorFromStaticStr: {0}")]
    ErrorFromStr(&'static str),
    #[error("unknown error")]
    UnknownError,
}

/// dropbox_backup_to_external_disk_lib::Result
///
/// `dropbox_backup_to_External_disk_lib::Result` is used with one parameter.
/// Instead of the regular Result with second parameter, that is always DropboxBackupToExternalDiskError.
pub type Result<T, E = Error> = core::result::Result<T, E>;
