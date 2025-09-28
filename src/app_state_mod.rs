// app_state_mod.rs

use crossplatform_path::CrossPathBuf;

use crate::error_mod::Result;

#[derive(Debug)]
pub struct AppConfig {
    pub path_list_ext_disk_base_path: CrossPathBuf,
    pub path_list_source_files: CrossPathBuf,
    pub path_list_destination_files: CrossPathBuf,
    pub path_list_source_folders: CrossPathBuf,
    pub path_list_destination_folders: CrossPathBuf,
    pub path_list_readonly_files: CrossPathBuf,
    pub path_list_for_download: CrossPathBuf,
    pub path_list_for_trash_files: CrossPathBuf,
    pub path_list_just_downloaded: CrossPathBuf,
    pub path_list_for_trash_folders: CrossPathBuf,
    pub path_list_for_create_folders: CrossPathBuf,
    pub path_list_for_change_time_files: CrossPathBuf,
}

/// This trait defines what functions must the bin project implement then the lib project can use them.  \
///
/// All IO must be defined inside the bin project: UI, env, file access.  \
/// That way the same lib project can be used from different bin: CLI, TUI, GUI, env, file, network,...  \
/// These methods will be available globally.  
pub trait AppStateMethods: Sync + Send {
    /// get encrypted authorization token from env var
    fn load_keys_from_io(&self) -> Result<(String, String)>;
    /// reference to app_config data
    fn ref_app_config(&self) -> &AppConfig;
    /// get locked Mutex
    fn lock_proba(&self) -> std::sync::MutexGuard<'_, String>;
}

/// Global variable to store the Application state.  \
///
/// Global variables are so complicated in Rust.  \
/// Read more: <https://www.sitepoint.com/rust-global-variables/>  \
/// I will use Multi-threaded Global Variable with Runtime Initialization and Interior Mutability, the most complicated and usable one.  \
/// All fields are private. Only the methods can be used globally.  \
/// Example how to use it:  \
///
/// ```rust ignore
/// fn global_app_state() -> &'static Box<dyn lib::AppStateMethods> {
///     lib::APP_STATE.get().expect("Error OnceCell must not be empty.")
/// }
/// println!("{}", global_app_state().ref_app_config().path_list_for_create_folders);
/// println!("{}", global_app_state().lock_proba());  
/// ```
pub static APP_STATE: once_cell::sync::OnceCell<Box<dyn AppStateMethods>> = once_cell::sync::OnceCell::new();

pub fn global_config() -> &'static AppConfig {
    global_app_state().ref_app_config()
}

/// Returns the global app state.
// Clippy warning here, but I don't know how to change this.
// for further information visit https://rust-lang.github.io/rust-clippy/master/index.html#borrowed_box
#[allow(clippy::borrowed_box)]
pub fn global_app_state() -> &'static Box<dyn AppStateMethods> {
    APP_STATE.get().expect("Bug: OnceCell must not be empty.")
}
