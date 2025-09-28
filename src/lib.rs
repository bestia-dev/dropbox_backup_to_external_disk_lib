// dropbox_backup_to_external_disk_lib/src/lib.rs

#![doc=include_str!("../README.md")]

mod app_state_mod;
mod compare_mod;
mod dropbox_api_token_with_oauth2_mod;
mod encrypt_decrypt_mod;
mod error_mod;
mod file_txt_mod;
mod local_disk_mod;
mod remote_dropbox_mod;
mod utils_mod;

// export public code to the bin project
pub use crate::app_state_mod::{global_app_state, global_config, AppConfig, AppStateMethods, APP_STATE};
pub use crate::compare_mod::{compare_files, compare_folders};
pub use crate::dropbox_api_token_with_oauth2_mod::dropbox_api_config_initialize;
pub use crate::error_mod::{Error, Result};
pub use crate::file_txt_mod::FileTxt;
pub use crate::local_disk_mod::{
    change_time_files, create_folders, list_local, move_local_files, read_only_remove, rename_local_files, trash_files, trash_folders,
};
pub use crate::remote_dropbox_mod::{download_from_list, download_one_file, encode_token, list_remote, test_connection};
pub use crate::utils_mod::{shorten_string, sort_string_lines};

/*
/// list and sync is the complete process for backup in one command
pub fn list_and_sync(ext_disk_base_path: &str, app_config: &'static AppConfig) {
    all_list_remote_and_local(ext_disk_base_path, app_config);
    press_enter_to_continue_timeout_5_sec();
    sync_only(app_config);
}

/// all list remote and local
pub fn all_list_remote_and_local(ext_disk_base_path: &str, app_config: &'static AppConfig) {
    let _hide_cursor_terminal = crate::start_hide_cursor_terminal();
    println!("{}{}dropbox_backup_to_external_disk_cli list_and_sync{}", at_line(1), *YELLOW, *RESET);
    ns_start("");
    // start 2 threads, first for remote list and second for local list
    use std::thread;
    let ext_disk_base_path = ext_disk_base_path.to_string();
    let handle_2 = thread::spawn(move || {
        println!("{}{}3 threads for source (remote files):{}", at_line(3), *GREEN, *RESET);
        // prints at rows 4,5,6 and 7,8,9
        list_remote(app_config);
    });
    let handle_1 = thread::spawn(move || {
        println!("{}{}1 thread for destination (local files):{}", at_line(12), *GREEN, *RESET);
        // prints at rows 13,14,15,16
        list_local(&ext_disk_base_path, app_config);
    });
    // wait for both threads to finish
    handle_1.join()?;
    handle_2.join()?;

    println!("{}{}", at_line(20), *CLEAR_LINE);
}

/// sync_only can be stopped with ctrl+c and then restarted if downloading takes lots of time.
/// No need to repeat the "list" that takes lots of times.
pub fn sync_only(app_config: &'static AppConfig) {
    println!("{}compare remote and local lists{}", *YELLOW, *RESET);
    compare_files(app_config);
    println!("{}move equal files{}", *YELLOW, *RESET);
    move_local_files(app_config);
    println!("{}rename equal files{}", *YELLOW, *RESET);
    rename_local_files(app_config);
    println!("{}move to trash from list{}", *YELLOW, *RESET);
    trash_files(app_config);
    press_enter_to_continue_timeout_5_sec();
    download_from_list(app_config);
}



 */
