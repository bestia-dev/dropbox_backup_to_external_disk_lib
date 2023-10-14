// app_state_mod.rs

use crate::error_mod::LibError;

/// This trait defines what functions must the bin project implement then the lib project can use them.  
/// All IO must be defined inside the bin project: UI, env, file access.  
/// That way the same lib project can be used from different bin: CLI, TUI, GUI, env, file, network,...  
/// These methods will be available globally.
pub trait AppStateTrait: Sync + Send {
    /// get encrypted authorization token from env var
    fn load_keys_from_io(&self) -> Result<(String, String), LibError>;
    /// get first field
    fn get_first_field(&self) -> String;
    /// set first field
    fn set_first_field(&mut self, value: String);
}

/// Global variable to store the Application state.  
/// Global variables are so complicated in Rust.  
/// Read more: https://www.sitepoint.com/rust-global-variables/  
/// I will use Multi-threaded Global Variable with Runtime Initialization and Interior Mutability, the most complicated and usable one.  
/// All fields are private. Only the methods can be used globally.  
/// Example how to use it: APP_STATE.get().unwrap().lock().unwrap().get_first_field()  
pub static APP_STATE: once_cell::sync::OnceCell<std::sync::Mutex<Box<dyn AppStateTrait>>> = once_cell::sync::OnceCell::new();
