// local_disk_mod.rs

//! Module contains all functions for local external disk.

use chrono::{DateTime, Utc};
use crossplatform_path::CrossPathBuf;
#[allow(unused_imports)]
use dropbox_content_hasher::DropboxContentHasher;

// type alias for better expressing coder intention,
// but programmatically identical to the underlying type
type ThreadName = String;

/* use log::error;
use std::io::Write;
use std::path;
use uncased::UncasedStr;
use unwrap::unwrap; */

use crate::{
    utils_mod::{println_to_ui_thread, println_to_ui_thread_with_thread_name},
    DropboxBackupToExternalDiskError, FileTxt,
};

/// The logic is in the LIB project, but all UI is in the CLI project.  \
///
/// They run on different threads and communicate.  \
/// It uses the global APP_STATE for all config data.  
pub fn list_local(
    ui_tx: std::sync::mpsc::Sender<(String, ThreadName)>,
    ext_disk_base_path: String,
    mut file_list_destination_files: FileTxt,
    mut file_list_destination_folders: FileTxt,
    mut file_list_destination_readonly_files: FileTxt,
) -> Result<(), DropboxBackupToExternalDiskError> {
    let list_local_start = std::time::Instant::now();

    // empty the file. I want all or nothing result here if the process is terminated prematurely.
    file_list_destination_files.empty()?;
    file_list_destination_folders.empty()?;
    file_list_destination_readonly_files.empty()?;

    // write data to a big string in memory (for my use-case it is >25 MB)
    let mut files_string = String::with_capacity(40_000_000);
    let mut folders_string = String::new();
    let mut readonly_files_string = String::new();
    use walkdir::WalkDir;

    let mut folder_count = 0;
    let mut file_count = 0;
    let mut last_send_ms = std::time::Instant::now();
    let walkdir_iterator = WalkDir::new(&ext_disk_base_path);
    for entry in walkdir_iterator {
        //let mut ns_started = ns_start("WalkDir entry start");
        let entry: walkdir::DirEntry = entry?;
        let path = entry.path();
        let str_path = path
            .to_str()
            .ok_or_else(|| DropboxBackupToExternalDiskError::ErrorFromStr("Error string is not path"))?;
        // I don't need the "base" folder in this list. The ext_disk_base_path always ends with slash.
        let str_path_wo_base = str_path.trim_start_matches(&ext_disk_base_path);
        // change windows style with backslash to Linux style with neutral crossplatform slash
        let str_path_wo_base = str_path_wo_base.replace(r#"\"#, "/");
        let str_path_wo_base = &str_path_wo_base;
        // path.is_dir() is slow. entry.file-type().is_dir() is fast
        if entry.file_type().is_dir() {
            if !str_path_wo_base.is_empty() {
                // avoid the temp_trash folder
                if !str_path_wo_base.starts_with("0_backup_temp") {
                    folders_string.push_str(&format!("{}\n", str_path_wo_base));
                    // don't print every folder, because print is slow. Check if 100ms passed
                    if last_send_ms.elapsed().as_millis() >= 100 {
                        println_to_ui_thread_with_thread_name(
                            &ui_tx,
                            format!("{file_count}: {}", crate::shorten_string(str_path_wo_base, 80)),
                            "L",
                        );

                        last_send_ms = std::time::Instant::now();
                    }
                    folder_count += 1;
                }
            }
        } else {
            // write csv tab delimited
            // metadata() in wsl/Linux is slow. Nothing to do here.
            if let Ok(metadata) = entry.metadata() {
                if !str_path_wo_base.starts_with("0_backup_temp") {
                    use chrono::offset::Utc;
                    use chrono::DateTime;
                    let datetime: DateTime<Utc> = metadata.modified().unwrap().into();

                    if metadata.permissions().readonly() {
                        readonly_files_string.push_str(&format!("{}\n", str_path_wo_base,));
                    }
                    files_string.push_str(&format!(
                        "{}\t{}\t{}\n",
                        str_path_wo_base,
                        datetime.format("%Y-%m-%dT%TZ"),
                        metadata.len()
                    ));

                    file_count += 1;
                }
            }
        }
    }

    // region: sort
    let files_sorted_string = crate::sort_string_lines(&files_string);
    let folders_sorted_string = crate::sort_string_lines(&folders_string);
    let readonly_files_sorted_string = crate::sort_string_lines(&readonly_files_string);
    // end region: sort
    file_list_destination_files.write_append_str(&files_sorted_string)?;
    file_list_destination_folders.write_append_str(&folders_sorted_string)?;
    file_list_destination_readonly_files.write_append_str(&readonly_files_sorted_string)?;

    println_to_ui_thread_with_thread_name(&ui_tx, format!("Local folder count: {folder_count}"), "L");
    println_to_ui_thread_with_thread_name(&ui_tx, format!("Local file count: {file_count}"), "L");
    println_to_ui_thread_with_thread_name(
        &ui_tx,
        format!("Local readonly count: {}", readonly_files_string.lines().count()),
        "L",
    );
    println_to_ui_thread_with_thread_name(
        &ui_tx,
        format!("Local duration in seconds: {}", list_local_start.elapsed().as_secs()),
        "L",
    );

    Ok(())
}

/// The backup files must not be readonly to allow copying the modified file from the remote.  \
///
/// The FileTxt is read+write. It is opened in the bin and not in lib, but it is manipulated only in lib.  
pub fn read_only_remove(
    ui_tx: std::sync::mpsc::Sender<String>,
    ext_disk_base_path: &CrossPathBuf,
    file_readonly_files: &mut FileTxt,
) -> Result<(), DropboxBackupToExternalDiskError> {
    let list_readonly_files = file_readonly_files.read_to_string()?;
    for string_path_for_readonly in list_readonly_files.lines() {
        let path_global_path_to_readonly = ext_disk_base_path.join_relative(string_path_for_readonly)?;
        // if path does not exist ignore
        if path_global_path_to_readonly.exists() {
            let mut perms = path_global_path_to_readonly.to_path_buf_current_os().metadata()?.permissions();
            if perms.readonly() {
                // note: on Unix platforms this results in the file being world writable
                // this is ok for backup
                #[allow(clippy::permissions_set_readonly_false)]
                perms.set_readonly(false);
                match std::fs::set_permissions(path_global_path_to_readonly.to_path_buf_current_os(), perms) {
                    Ok(_) => println_to_ui_thread(&ui_tx, string_path_for_readonly.to_string()),
                    Err(_err) => println_to_ui_thread(&ui_tx, format!("Error set_permissions readonly: {string_path_for_readonly}")),
                }
            }
        }
    }
    file_readonly_files.empty()?;
    Ok(())
}

/// Change time of files.  
pub fn change_time_files(
    ui_tx: std::sync::mpsc::Sender<String>,
    ext_disk_base_path: &CrossPathBuf,
    file_list_for_change_time_files: &mut FileTxt,
) -> Result<(), DropboxBackupToExternalDiskError> {
    let list_for_change_time_files = file_list_for_change_time_files.read_to_string()?;
    if list_for_change_time_files.is_empty() {
        println_to_ui_thread(&ui_tx, "list_for_change_time_files is empty".to_string());
    } else {
        for line in list_for_change_time_files.lines() {
            let vec_line: Vec<&str> = line.split("\t").collect();
            let path = vec_line[0];
            let datetime = vec_line[1];
            let path_global_path = ext_disk_base_path.join_relative(path)?;
            println_to_ui_thread(&ui_tx, path_global_path.to_string());
            let modified = filetime::FileTime::from_system_time(humantime::parse_rfc3339(datetime).unwrap());
            filetime::set_file_mtime(path_global_path.to_path_buf_current_os(), modified).unwrap();
        }
        file_list_for_change_time_files.empty()?;
    }
    Ok(())
}

/// Create new empty folders.  
pub fn create_folders(
    ui_tx: std::sync::mpsc::Sender<String>,
    ext_disk_base_path: &CrossPathBuf,
    file_list_for_create_folders: &mut FileTxt,
) -> Result<(), DropboxBackupToExternalDiskError> {
    let list_for_create_folders = file_list_for_create_folders.read_to_string()?;
    if list_for_create_folders.is_empty() {
        println_to_ui_thread(&ui_tx, "list_for_create_folders is empty".to_string());
    } else {
        for string_path in list_for_create_folders.lines() {
            let path_global_path = ext_disk_base_path.join_relative(string_path)?;
            // if path exists ignore
            if !path_global_path.exists() {
                println_to_ui_thread(&ui_tx, path_global_path.to_string());
                path_global_path.create_dir_all()?;
            }
        }
        file_list_for_create_folders.empty()?;
    }
    Ok(())
}

/// Files are often moved.  \
///
/// After compare, the same file (with different path or name) will be in the list_for_trash_files and in the list_for_download.  \
/// First for every trash line, we search list_for_download for same name, size and modified.  \
/// If they are equal move, else nothing: it will be trashed and downloaded eventually.  \
/// Remove also the lines in files list_for_trash_files and list_for_download.  
pub fn move_local_files(
    ui_tx: std::sync::mpsc::Sender<String>,
    ext_disk_base_path: &CrossPathBuf,
    file_list_for_trash_files: &mut FileTxt,
    file_list_for_download: &mut FileTxt,
) -> Result<(), DropboxBackupToExternalDiskError> {
    let list_for_trash_files = file_list_for_trash_files.read_to_string()?;
    let list_for_download = file_list_for_download.read_to_string()?;
    let mut vec_list_for_trash_files: Vec<&str> = list_for_trash_files.lines().collect();
    let mut vec_list_for_download: Vec<&str> = list_for_download.lines().collect();

    match move_local_files_internal_by_name(
        ui_tx.clone(),
        ext_disk_base_path,
        &mut vec_list_for_trash_files,
        &mut vec_list_for_download,
    ) {
        Ok(()) => {
            // in case all is ok, write actual situation to disk and continue
            file_list_for_trash_files.empty()?;
            file_list_for_trash_files.write_append_str(&vec_list_for_trash_files.join("\n"))?;
            file_list_for_download.empty()?;
            file_list_for_download.write_append_str(&vec_list_for_download.join("\n"))?;
        }
        Err(err) => {
            // also in case of error, write the actual situation to disk and return error
            file_list_for_trash_files.empty()?;
            file_list_for_trash_files.write_append_str(&vec_list_for_trash_files.join("\n"))?;
            file_list_for_download.empty()?;
            file_list_for_download.write_append_str(&vec_list_for_download.join("\n"))?;
            return Err(err);
        }
    }
    Ok(())
}

/// Files are often renamed.  \
///
/// After compare, the same file (with different path or name) will be in the list_for_trash_files and in the list_for_download.  \
/// First for every trash line, we search list_for_download for same size and modified.  \
/// If found, compare content_hash and calculate local_content_hash.  \
/// If they are equal rename, else nothing: it will be trashed and downloaded eventually.  \
/// Remove also the lines in files list_for_trash_files and list_for_download.  
pub fn rename_local_files(
    ui_tx: std::sync::mpsc::Sender<String>,
    ext_disk_base_path: &CrossPathBuf,
    file_list_for_trash_files: &mut FileTxt,
    file_list_for_download: &mut FileTxt,
) -> Result<(), DropboxBackupToExternalDiskError> {
    let list_for_trash_files = file_list_for_trash_files.read_to_string()?;
    let list_for_download = file_list_for_download.read_to_string()?;
    let mut vec_list_for_trash: Vec<&str> = list_for_trash_files.lines().collect();
    let mut vec_list_for_download: Vec<&str> = list_for_download.lines().collect();

    match rename_local_files_internal_by_hash(ui_tx, ext_disk_base_path, &mut vec_list_for_trash, &mut vec_list_for_download) {
        Ok(()) => {
            // in case all is ok, write actual situation to disk
            file_list_for_trash_files.empty()?;
            file_list_for_trash_files.write_append_str(&vec_list_for_trash.join("\n"))?;
            file_list_for_download.empty()?;
            file_list_for_download.write_append_str(&vec_list_for_download.join("\n"))?;
        }
        Err(err) => {
            // also in case of error, write the actual situation to disk
            file_list_for_trash_files.empty()?;
            file_list_for_trash_files.write_append_str(&vec_list_for_trash.join("\n"))?;
            file_list_for_download.empty()?;
            file_list_for_download.write_append_str(&vec_list_for_download.join("\n"))?;
            return Err(err);
        }
    }
    Ok(())
}

// internal because of catching errors
fn move_local_files_internal_by_name(
    ui_tx: std::sync::mpsc::Sender<String>,
    ext_disk_base_path: &CrossPathBuf,
    vec_list_for_trash_files: &mut Vec<&str>,
    vec_list_for_download: &mut Vec<&str>,
) -> Result<(), DropboxBackupToExternalDiskError> {
    let mut count_moved = 0;
    // it is not possible to remove an element when iterating a Vec
    // I will iterate by a clone, so I can remove an element in the original Vec
    let vec_list_for_trash_clone = vec_list_for_trash_files.clone();
    let vec_list_for_download_clone = vec_list_for_download.clone();
    let mut last_send_ms = std::time::Instant::now();

    for line_for_trash_files in vec_list_for_trash_clone.iter() {
        let split_line_for_trash: Vec<&str> = line_for_trash_files.split("\t").collect();
        let string_path_for_trash_files = split_line_for_trash[0];
        let path_global_to_trash_files = ext_disk_base_path.join_relative(string_path_for_trash_files)?;
        // if path does not exist ignore, probably it has moved or trashed earlier
        if path_global_to_trash_files.exists() {
            let modified_for_trash_files = split_line_for_trash[1];
            let size_for_trash_files = split_line_for_trash[2];
            let file_name_for_trash_files = path_global_to_trash_files.file_name()?;

            // search in list_for_download for possible candidates
            // first try exact match with name, date, size because it is fast
            for line_for_download in vec_list_for_download_clone.iter() {
                // Every 1 second write a dot, to see it still works like a progress bar
                if last_send_ms.elapsed().as_millis() >= 1000 {
                    // this is a special character fpr a progress bar
                    println_to_ui_thread(&ui_tx, ".".to_string());

                    last_send_ms = std::time::Instant::now();
                }
                let split_line_for_download: Vec<&str> = line_for_download.split("\t").collect();
                let string_path_for_download = split_line_for_download[0];
                let modified_for_download = split_line_for_download[1];
                let size_for_download = split_line_for_download[2];
                let path_global_to_download = ext_disk_base_path.join_relative(string_path_for_download)?;
                let file_name_for_download = path_global_to_download.file_name()?;

                let modified_for_trash_files: DateTime<Utc> = DateTime::parse_from_rfc3339(modified_for_trash_files)
                    .expect("Bug: datetime must be correct")
                    .into();
                let modified_for_download: DateTime<Utc> = DateTime::parse_from_rfc3339(modified_for_download)
                    .expect("Bug: datetime must be correct")
                    .into();
                if chrono::Duration::from(modified_for_trash_files - modified_for_download).abs() < chrono::Duration::seconds(2)
                    && size_for_trash_files == size_for_download
                    && file_name_for_trash_files == file_name_for_download
                {
                    move_internal(&ui_tx, &path_global_to_trash_files, &path_global_to_download)?;
                    // remove the lines from the original mut Vec
                    vec_list_for_trash_files.retain(|line| line != line_for_trash_files);
                    vec_list_for_download.retain(|line| line != line_for_download);

                    count_moved += 1;
                    break;
                }
            }
        }
    }

    println_to_ui_thread(&ui_tx, format!("moved by name: {}", count_moved));
    Ok(())
}

// Internal because of catching errors.
fn rename_local_files_internal_by_hash(
    ui_tx: std::sync::mpsc::Sender<String>,
    ext_disk_base_path: &CrossPathBuf,
    vec_list_for_trash_files: &mut Vec<&str>,
    vec_list_for_download: &mut Vec<&str>,
) -> Result<(), DropboxBackupToExternalDiskError> {
    let mut count_moved = 0;
    // it is not possible to remove an element when iterating a Vec
    // I will iterate by a clone, so I can remove an element in the original Vec
    let vec_list_for_trash_clone = vec_list_for_trash_files.clone();
    let vec_list_for_download_clone = vec_list_for_download.clone();
    let mut last_send_ms = std::time::Instant::now();

    for line_for_trash_files in vec_list_for_trash_clone.iter() {
        let split_line_for_trash: Vec<&str> = line_for_trash_files.split("\t").collect();
        let string_path_for_trash_files = split_line_for_trash[0];
        let path_global_to_trash_files = ext_disk_base_path.join_relative(string_path_for_trash_files)?;
        // if path does not exist ignore, probably it eas moved or trashed earlier
        if path_global_to_trash_files.exists() {
            let modified_for_trash_files = split_line_for_trash[1];
            let size_for_trash_files = split_line_for_trash[2];

            for line_for_download in vec_list_for_download_clone.iter() {
                // Every 1 second write a dot, to see it still works like a progress bar
                if last_send_ms.elapsed().as_millis() >= 1000 {
                    // this is a special character fpr a progress bar
                    println_to_ui_thread(&ui_tx, ".".to_string());

                    last_send_ms = std::time::Instant::now();
                }
                let split_line_for_download: Vec<&str> = line_for_download.split("\t").collect();
                let string_path_for_download = split_line_for_download[0];
                let modified_for_download = split_line_for_download[1];
                let size_for_download = split_line_for_download[2];
                let remote_content_hash = split_line_for_download[3];
                let path_global_to_download = ext_disk_base_path.join_relative(string_path_for_download)?;

                let modified_for_trash_files: DateTime<Utc> = DateTime::parse_from_rfc3339(modified_for_trash_files)
                    .expect("Bug: datetime must be correct")
                    .into();
                let modified_for_download: DateTime<Utc> = DateTime::parse_from_rfc3339(modified_for_download)
                    .expect("Bug: datetime must be correct")
                    .into();
                if chrono::Duration::from(modified_for_trash_files - modified_for_download).abs() < chrono::Duration::seconds(2)
                    && size_for_trash_files == size_for_download
                {
                    // same size and date. Let's check the content_hash to be sure.
                    let local_content_hash = format!(
                        "{:x}",
                        DropboxContentHasher::hash_file(path_global_to_trash_files.to_path_buf_current_os())?
                    );

                    if local_content_hash == remote_content_hash {
                        move_internal(&ui_tx, &path_global_to_trash_files, &path_global_to_download)?;
                        // remove the lines from the original mut Vec
                        vec_list_for_trash_files.retain(|line| line != line_for_trash_files);
                        vec_list_for_download.retain(|line| line != line_for_download);
                        count_moved += 1;
                        break;
                    }
                }
            }
        }
    }

    println_to_ui_thread(&ui_tx, format!("Renamed: {}", count_moved));
    Ok(())
}

/// Internal code to move file.  
fn move_internal(
    ui_tx: &std::sync::mpsc::Sender<String>,
    path_global_to_trash: &CrossPathBuf,
    path_global_for_download: &CrossPathBuf,
) -> Result<(), DropboxBackupToExternalDiskError> {
    let move_from = path_global_to_trash;
    let move_to = path_global_for_download;
    println_to_ui_thread(ui_tx, format!("move {}  ->  {}", &move_from, &move_to));
    move_to.create_dir_all_for_file()?;
    if move_to.exists() {
        let mut perms = std::fs::metadata(move_to.to_path_buf_current_os())?.permissions();
        if perms.readonly() {
            // note: on Unix platforms this results in the file being world writable
            // this is ok for backup
            #[allow(clippy::permissions_set_readonly_false)]
            perms.set_readonly(false);
            std::fs::set_permissions(move_to.to_path_buf_current_os(), perms)?;
        }
    }
    if move_from.exists() {
        let mut perms = std::fs::metadata(move_from.to_path_buf_current_os())?.permissions();
        if perms.readonly() {
            // note: on Unix platforms this results in the file being world writable
            // this is ok for backup
            #[allow(clippy::permissions_set_readonly_false)]
            perms.set_readonly(false);
            std::fs::set_permissions(move_from.to_path_buf_current_os(), perms)?;
        }
    }
    std::fs::rename(move_from.to_path_buf_current_os(), move_to.to_path_buf_current_os())?;
    Ok(())
}

/// Move to trash folder the files from list_for_trash_files.  \
///
/// Ignore if the file does not exist anymore.  
pub fn trash_files(
    ui_tx: std::sync::mpsc::Sender<String>,
    ext_disk_base_path: &CrossPathBuf,
    file_list_for_trash_files: &mut FileTxt,
) -> Result<(), DropboxBackupToExternalDiskError> {
    let list_for_trash_files = file_list_for_trash_files.read_to_string()?;
    let mut vec_list_for_trash_files: Vec<&str> = list_for_trash_files.lines().collect();

    match trash_files_internal(ui_tx.clone(), ext_disk_base_path, &mut vec_list_for_trash_files) {
        Ok(()) => {
            // in case all is ok, write actual situation to disk and continue
            file_list_for_trash_files.empty()?;
            file_list_for_trash_files.write_append_str(&vec_list_for_trash_files.join("\n"))?;
        }
        Err(err) => {
            // also in case of error, write the actual situation to disk and return error
            file_list_for_trash_files.empty()?;
            file_list_for_trash_files.write_append_str(&vec_list_for_trash_files.join("\n"))?;
            return Err(err);
        }
    }
    Ok(())
}

/// Internal function.  
fn trash_files_internal(
    ui_tx: std::sync::mpsc::Sender<String>,
    ext_disk_base_path: &CrossPathBuf,
    vec_list_for_trash_files: &mut Vec<&str>,
) -> Result<(), DropboxBackupToExternalDiskError> {
    let vec_list_for_trash_clone = vec_list_for_trash_files.clone();
    let now_string = chrono::Local::now().format("trash_%Y-%m-%d_%H-%M-%S").to_string();
    // the trash folder will be inside DropBoxBackup because of permissions
    let base_trash_path = ext_disk_base_path.join_relative("0_backup_temp")?.join_relative(&now_string)?;
    base_trash_path.create_dir_all()?;
    //move the files in the same directory structure
    for line_path_for_trash_files in vec_list_for_trash_clone.iter() {
        let line: Vec<&str> = line_path_for_trash_files.split("\t").collect();
        let string_path_for_trash_files = line[0];
        let path_move_from = ext_disk_base_path.join_relative(string_path_for_trash_files)?;
        // move to trash if file exists. Nothing if it does not exist, maybe is deleted when moved or in a move to trash before.
        if path_move_from.exists() {
            let path_move_to = base_trash_path.join_relative(string_path_for_trash_files)?;
            println_to_ui_thread(&ui_tx, format!("{}", path_move_from));
            path_move_to.create_dir_all_for_file()?;
            std::fs::rename(path_move_from.to_path_buf_current_os(), path_move_to.to_path_buf_current_os())?;
        }
        vec_list_for_trash_files.retain(|line| line != line_path_for_trash_files);
    }
    Ok(())
}

/// Move to trash folder the folders from list_for_trash_folders.
pub fn trash_folders(
    ui_tx: std::sync::mpsc::Sender<String>,
    ext_disk_base_path: &CrossPathBuf,
    file_list_for_trash_folders: &mut FileTxt,
) -> Result<(), DropboxBackupToExternalDiskError> {
    let list_for_trash_folders = file_list_for_trash_folders.read_to_string()?;
    let mut vec_list_for_trash_folders: Vec<&str> = list_for_trash_folders.lines().collect();
    let vec_list_for_trash_clone = vec_list_for_trash_folders.clone();
    let now_string = chrono::Local::now().format("trash_%Y-%m-%d_%H-%M-%S").to_string();
    let base_trash_path_folders = ext_disk_base_path.join_relative("0_backup_temp")?.join_relative(&now_string)?;
    base_trash_path_folders.create_dir_all()?;
    for string_path_for_trash_folders in vec_list_for_trash_clone.iter() {
        let path_move_from = ext_disk_base_path.join_relative(string_path_for_trash_folders)?;
        // move to trash if file exists. Nothing if it does not exist, maybe is deleted when moved or in a move to trash before.
        if path_move_from.exists() {
            let path_move_to = base_trash_path_folders.join_relative(string_path_for_trash_folders)?;
            println_to_ui_thread(&ui_tx, format!("{}", path_move_from));
            path_move_to.create_dir_all_for_file()?;
            std::fs::rename(path_move_from.to_path_buf_current_os(), path_move_to.to_path_buf_current_os())?;
        }
        vec_list_for_trash_folders.retain(|line| line != string_path_for_trash_folders);
    }
    file_list_for_trash_folders.empty()?;
    file_list_for_trash_folders.write_append_str(&vec_list_for_trash_folders.join("\n"))?;

    Ok(())
}
