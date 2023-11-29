// local_disk_mod.rs

//! Module contains all functions for local external disk.

use std::path::Path;

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
    FileTxt, LibError,
};

/// the logic is in the LIB project, but all UI is in the CLI project
/// they run on different threads and communicate
/// It uses the global APP_STATE for all config data
pub fn list_local(
    ui_tx: std::sync::mpsc::Sender<(String, ThreadName)>,
    base_path: String,
    mut file_list_destination_files: FileTxt,
    mut file_list_destination_folders: FileTxt,
    mut file_list_destination_readonly_files: FileTxt,
) -> Result<(), LibError> {
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
    for entry in WalkDir::new(&base_path) {
        //let mut ns_started = ns_start("WalkDir entry start");
        let entry: walkdir::DirEntry = entry?;
        let path = entry.path();
        let str_path = path.to_str().ok_or_else(|| LibError::ErrorFromStr("Error string is not path"))?;
        // path.is_dir() is slow. entry.file-type().is_dir() is fast
        if entry.file_type().is_dir() {
            // I don't need the "base" folder in this list
            if !str_path.trim_start_matches(&base_path).is_empty() {
                folders_string.push_str(&format!("{}\n", str_path.trim_start_matches(&base_path),));
                // TODO: don't print every folder, because print is slow. Check if 100ms passed
                if last_send_ms.elapsed().as_millis() >= 100 {
                    println_to_ui_thread_with_thread_name(&ui_tx, format!("{file_count}: {}", crate::shorten_string(str_path.trim_start_matches(&base_path), 80)), format!("L0"));

                    last_send_ms = std::time::Instant::now();
                }
                folder_count += 1;
            }
        } else {
            // write csv tab delimited
            // metadata() in wsl/Linux is slow. Nothing to do here.
            if let Ok(metadata) = entry.metadata() {
                use chrono::offset::Utc;
                use chrono::DateTime;
                let datetime: DateTime<Utc> = metadata.modified().unwrap().into();

                if metadata.permissions().readonly() {
                    readonly_files_string.push_str(&format!("{}\n", str_path.trim_start_matches(&base_path),));
                }
                files_string.push_str(&format!("{}\t{}\t{}\n", str_path.trim_start_matches(&base_path), datetime.format("%Y-%m-%dT%TZ"), metadata.len()));

                file_count += 1;
            }
        }
    }

    println_to_ui_thread_with_thread_name(&ui_tx, format!("local_folder_count: {folder_count}"), "L0".to_string());

    // region: sort
    let files_sorted_string = crate::sort_string_lines(&files_string);
    let folders_sorted_string = crate::sort_string_lines(&folders_string);
    let readonly_files_sorted_string = crate::sort_string_lines(&readonly_files_string);
    // end region: sort
    file_list_destination_files.write_str(&files_sorted_string)?;
    file_list_destination_folders.write_str(&folders_sorted_string)?;
    file_list_destination_readonly_files.write_str(&readonly_files_sorted_string)?;

    println_to_ui_thread_with_thread_name(&ui_tx, "All lists stored in files.".to_string(), "L0".to_string());

    Ok(())
}

/// The backup files must not be readonly to allow copying the modified file from the remote.
/// The FileTxt is read+write. It is opened in the bin and not in lib, but it is manipulated only in lib.
pub fn read_only_remove(ui_tx: std::sync::mpsc::Sender<String>, base_path: &Path, file_destination_readonly_files: &mut FileTxt) -> Result<(), LibError> {
    let list_destination_readonly_files = file_destination_readonly_files.read_to_string()?;
    let mut warning_shown_already = false;
    let mut there_is_a_readonly_files = false;
    for string_path_for_readonly in list_destination_readonly_files.lines() {
        let path_global_path_to_readonly = base_path.join(string_path_for_readonly.trim_start_matches("/"));
        // if path does not exist ignore
        if path_global_path_to_readonly.exists() {
            let mut perms = path_global_path_to_readonly.metadata()?.permissions();
            if perms.readonly() == true {
                perms.set_readonly(false);
                match std::fs::set_permissions(&path_global_path_to_readonly, perms) {
                    Ok(_) => println_to_ui_thread(&ui_tx, format!("{string_path_for_readonly}")),
                    Err(_err) => {
                        there_is_a_readonly_files = true;
                        if !warning_shown_already {
                            warning_shown_already = true;
                            let warning_wsl_cannot_change_attr_in_win = "Warning!!! 
From WSL it is not possible to change readonly attributes in Windows folders. 
You have to do it manually in PowerShell.";
                            println_to_ui_thread(&ui_tx, format!("{warning_wsl_cannot_change_attr_in_win}"));
                        }
                        // from WSL Debian I cannot change the readonly flag on the external disk mounted in Windows
                        // I get the error: IoError: Operation not permitted (os error 1)
                        // Instead I will return the commands to run manually in powershell for Windows
                        // change /mnt/e/ into e:
                        if path_global_path_to_readonly.starts_with("/mnt/") {
                            let win_path = path_global_path_to_readonly.to_string_lossy().trim_start_matches("/mnt/").to_string();
                            // replace only the first / with :/
                            let win_path = win_path.replacen("/", ":/", 1);
                            // replace all / with \
                            let win_path = win_path.replace("/", r#"\"#);
                            println_to_ui_thread(&ui_tx, format!("Set-ItemProperty -Path \"{win_path}\" -Name IsReadOnly -Value $false"));
                        }
                    }
                }
            }
        }
    }
    if !there_is_a_readonly_files {
        file_destination_readonly_files.empty()?;
        println_to_ui_thread(&ui_tx, format!("All files are now not-readonly."));
    } else {
        println_to_ui_thread(&ui_tx, format!("There is still some readonly files. Rerun the command."));
    }
    Ok(())
}

/// create new empty folders
pub fn create_folders(ui_tx: std::sync::mpsc::Sender<String>, base_path: &Path, file_list_for_create_folders: &mut FileTxt) -> Result<(), LibError> {
    let list_for_create_folders = file_list_for_create_folders.read_to_string()?;
    if list_for_create_folders.is_empty() {
        println_to_ui_thread(&ui_tx, format!("list_for_create_folders is empty"));
    } else {
        for string_path in list_for_create_folders.lines() {
            let path_global_path = base_path.join(string_path.trim_start_matches("/"));
            // if path exists ignore
            if !path_global_path.exists() {
                println_to_ui_thread(&ui_tx, path_global_path.to_string_lossy().to_string());
                std::fs::create_dir_all(path_global_path)?;
            }
        }
        file_list_for_create_folders.empty()?;
    }
    Ok(())
}

/// Files are often moved or renamed
/// After compare, the same file (with different path or name) will be in the list_for_trash and in the list_for_download.
/// First for every trash line, we search list_for_download for same size and modified.
/// If found, get the remote_metadata with content_hash and calculate local_content_hash.
/// If they are equal move or rename, else nothing: it will be trashed and downloaded eventually.
/// Remove also the lines in files list_for_trash and list_for_download.
pub fn move_or_rename_local_files(ui_tx: std::sync::mpsc::Sender<String>, ext_disk_base_path: &Path, file_list_for_trash: &mut FileTxt, file_list_for_download: &mut FileTxt) -> Result<(), LibError> {
    let list_for_trash = file_list_for_trash.read_to_string()?;
    let list_for_download = file_list_for_download.read_to_string()?;
    let mut vec_list_for_trash: Vec<&str> = list_for_trash.lines().collect();
    let mut vec_list_for_download: Vec<&str> = list_for_download.lines().collect();

    match move_or_rename_local_files_internal_by_name(ui_tx.clone(), ext_disk_base_path, &mut vec_list_for_trash, &mut vec_list_for_download) {
        Ok(()) => {
            // in case all is ok, write actual situation to disk and continue
            file_list_for_trash.write_str(&vec_list_for_trash.join("\n"))?;
            file_list_for_download.write_str(&vec_list_for_download.join("\n"))?;
        }
        Err(err) => {
            // also in case of error, write the actual situation to disk and return error
            file_list_for_trash.write_str(&vec_list_for_trash.join("\n"))?;
            file_list_for_download.write_str(&vec_list_for_download.join("\n"))?;
            return Err(err);
        }
    }

    let list_for_trash = file_list_for_trash.read_to_string()?;
    let list_for_download = file_list_for_download.read_to_string()?;
    let mut vec_list_for_trash: Vec<&str> = list_for_trash.lines().collect();
    let mut vec_list_for_download: Vec<&str> = list_for_download.lines().collect();

    match move_or_rename_local_files_internal_by_hash(ui_tx, ext_disk_base_path, &mut vec_list_for_trash, &mut vec_list_for_download) {
        Ok(()) => {
            // in case all is ok, write actual situation to disk
            file_list_for_trash.write_str(&vec_list_for_trash.join("\n"))?;
            file_list_for_download.write_str(&vec_list_for_download.join("\n"))?;
        }
        Err(err) => {
            // also in case of error, write the actual situation to disk
            file_list_for_trash.write_str(&vec_list_for_trash.join("\n"))?;
            file_list_for_download.write_str(&vec_list_for_download.join("\n"))?;
            return Err(err);
        }
    }
    Ok(())
}

// internal because of catching errors
fn move_or_rename_local_files_internal_by_name(
    ui_tx: std::sync::mpsc::Sender<String>,
    ext_disk_base_path: &Path,
    vec_list_for_trash: &mut Vec<&str>,
    vec_list_for_download: &mut Vec<&str>,
) -> Result<(), LibError> {
    let mut count_moved = 0;
    // it is not possible to remove an element when iterating a Vec
    // I will iterate by a clone, so I can remove an element in the original Vec
    let vec_list_for_trash_clone = vec_list_for_trash.clone();
    let vec_list_for_download_clone = vec_list_for_download.clone();
    let mut last_send_ms = std::time::Instant::now();

    for line_for_trash in vec_list_for_trash_clone.iter() {
        let vec_line_for_trash: Vec<&str> = line_for_trash.split("\t").collect();
        let string_path_for_trash = vec_line_for_trash[0];
        let path_global_to_trash = ext_disk_base_path.join(string_path_for_trash.trim_start_matches("/"));
        // if path does not exist ignore, probably it eas moved or trashed earlier
        if path_global_to_trash.exists() {
            let modified_for_trash = vec_line_for_trash[1];
            let size_for_trash = vec_line_for_trash[2];
            let file_name_for_trash = string_path_for_trash
                .split("/")
                .collect::<Vec<&str>>()
                .last()
                .expect("Bug: file_name_for_trash must be splitted and not empty")
                .to_string();

            // search in list_for_download for possible candidates
            // first try exact match with name, date, size because it is fast
            for line_for_download in vec_list_for_download_clone.iter() {
                // Every 1 second write a dot, to see it still works like a progress bar
                if last_send_ms.elapsed().as_millis() >= 1000 {
                    // this is a special character fpr a progress bar
                    println_to_ui_thread(&ui_tx, format!("."));

                    last_send_ms = std::time::Instant::now();
                }
                let vec_line_for_download: Vec<&str> = line_for_download.split("\t").collect();
                let string_path_for_download = vec_line_for_download[0];
                let modified_for_download = vec_line_for_download[1];
                let size_for_download = vec_line_for_download[2];
                let file_name_for_download = string_path_for_download
                    .split("/")
                    .collect::<Vec<&str>>()
                    .last()
                    .expect("Bug: file_name_for_download must be splitted and not empty")
                    .to_string();
                let path_global_to_download = ext_disk_base_path.join(string_path_for_download.trim_start_matches("/"));

                if modified_for_trash == modified_for_download && size_for_trash == size_for_download && file_name_for_trash == file_name_for_download {
                    move_internal(&ui_tx, &path_global_to_trash, &path_global_to_download)?;
                    // remove the lines from the original mut Vec
                    vec_list_for_trash.retain(|line| line != line_for_trash);
                    vec_list_for_download.retain(|line| line != line_for_download);

                    count_moved += 1;
                    break;
                }
            }
        }
    }

    println_to_ui_thread(&ui_tx, format!("moved or renamed by name: {}", count_moved));
    Ok(())
}

// internal because of catching errors
fn move_or_rename_local_files_internal_by_hash(
    ui_tx: std::sync::mpsc::Sender<String>,
    ext_disk_base_path: &Path,
    vec_list_for_trash: &mut Vec<&str>,
    vec_list_for_download: &mut Vec<&str>,
) -> Result<(), LibError> {
    let mut count_moved = 0;
    // it is not possible to remove an element when iterating a Vec
    // I will iterate by a clone, so I can remove an element in the original Vec
    let vec_list_for_trash_clone = vec_list_for_trash.clone();
    let vec_list_for_download_clone = vec_list_for_download.clone();
    let mut last_send_ms = std::time::Instant::now();

    for line_for_trash in vec_list_for_trash_clone.iter() {
        let vec_line_for_trash: Vec<&str> = line_for_trash.split("\t").collect();
        let string_path_for_trash = vec_line_for_trash[0];
        let path_global_to_trash = ext_disk_base_path.join(string_path_for_trash.trim_start_matches("/"));
        // if path does not exist ignore, probably it eas moved or trashed earlier
        if path_global_to_trash.exists() {
            let modified_for_trash = vec_line_for_trash[1];
            let size_for_trash = vec_line_for_trash[2];

            for line_for_download in vec_list_for_download_clone.iter() {
                // Every 1 second write a dot, to see it still works like a progress bar
                if last_send_ms.elapsed().as_millis() >= 1000 {
                    // this is a special character fpr a progress bar
                    println_to_ui_thread(&ui_tx, format!("."));

                    last_send_ms = std::time::Instant::now();
                }
                let vec_line_for_download: Vec<&str> = line_for_download.split("\t").collect();
                let string_path_for_download = vec_line_for_download[0];
                let modified_for_download = vec_line_for_download[1];
                let size_for_download = vec_line_for_download[2];
                let path_global_to_download = ext_disk_base_path.join(string_path_for_download.trim_start_matches("/"));

                if modified_for_trash == modified_for_download && size_for_trash == size_for_download {
                    // same size and date. Let's check the content_hash to be sure.
                    let local_content_hash = format!("{:x}", DropboxContentHasher::hash_file(&path_global_to_trash)?);
                    let remote_content_hash = get_content_hash(string_path_for_download)?;

                    if local_content_hash == remote_content_hash {
                        move_internal(&ui_tx, &path_global_to_trash, &path_global_to_download)?;
                        // remove the lines from the original mut Vec
                        vec_list_for_trash.retain(|line| line != line_for_trash);
                        vec_list_for_download.retain(|line| line != line_for_download);
                        count_moved += 1;
                        break;
                    }
                }
            }
        }
    }

    println_to_ui_thread(&ui_tx, format!("moved or renamed: {}", count_moved));
    Ok(())
}

/// internal code to move file
fn move_internal(ui_tx: &std::sync::mpsc::Sender<String>, path_global_to_trash: &Path, path_global_for_download: &Path) -> Result<(), LibError> {
    let move_from = path_global_to_trash;
    let move_to = path_global_for_download;
    println_to_ui_thread(&ui_tx, format!("move {}  ->  {}", &move_from.to_string_lossy(), &move_to.to_string_lossy()));

    let parent = Path::parent(Path::new(&move_to)).expect("Bug: Parent path must exist.");
    if !parent.exists() {
        std::fs::create_dir_all(&parent)?;
    }
    if Path::new(&move_to).exists() {
        let mut perms = std::fs::metadata(&move_to)?.permissions();
        if perms.readonly() == true {
            perms.set_readonly(false);
            std::fs::set_permissions(&move_to, perms)?;
        }
    }
    if Path::new(&move_from).exists() {
        let mut perms = std::fs::metadata(&move_from)?.permissions();
        if perms.readonly() == true {
            perms.set_readonly(false);
            std::fs::set_permissions(&move_from, perms)?;
        }
    }
    std::fs::rename(&move_from, &move_to)?;
    Ok(())
}

fn get_content_hash(path_for_download: &str) -> Result<String, LibError> {
    let token = crate::remote_dropbox_mod::get_authorization_token()?;
    let client = dropbox_sdk::default_client::UserAuthDefaultClient::new(token);
    Ok(crate::remote_dropbox_mod::remote_content_hash(path_for_download, &client).expect("Bug: dropbox metadata must have hash."))
}

/*

/// Move to trash folder the files from list_for_trash.
/// Ignore if the file does not exist anymore.
pub fn trash_from_list(app_config: &'static AppConfig) {
    let base_local_path = fs::read_to_string(app_config.path_list_ext_disk_base_path).unwrap();
    let path_list_local_files = app_config.path_list_destination_files;
    trash_from_list_internal(&base_local_path, app_config.path_list_for_trash, path_list_local_files);
}

/// internal
pub fn trash_from_list_internal(base_local_path: &str, path_list_for_trash: &str, path_list_local_files: &str) {
    let list_for_trash = fs::read_to_string(path_list_for_trash).unwrap();
    if list_for_trash.is_empty() {
        println!("{}: 0", path_list_for_trash);
    } else {
        let now_string = chrono::Local::now().format("trash_%Y-%m-%d_%H-%M-%S").to_string();
        let base_trash_path = format!("{}_{}", base_local_path, &now_string);
        if !path::Path::new(&base_trash_path).exists() {
            fs::create_dir_all(&base_trash_path).unwrap();
        }
        //move the files in the same directory structure
        for line_path_for_trash in list_for_trash.lines() {
            let line: Vec<&str> = line_path_for_trash.split("\t").collect();
            let string_path_for_trash = line[0];
            let move_from = format!("{}{}", base_local_path, string_path_for_trash);
            let path_move_from = path::Path::new(&move_from);
            // move to trash if file exists. Nothing if it does not exist, maybe is deleted when moved or in a move_to_trash before.
            if path_move_from.exists() {
                let move_to = format!("{}{}", base_trash_path, string_path_for_trash);
                println!("{}  ->  {}", move_from, move_to);
                let parent = unwrap!(path::Path::parent(path::Path::new(&move_to)));
                if !parent.exists() {
                    fs::create_dir_all(&parent).unwrap();
                }
                unwrap!(fs::rename(&move_from, &move_to));
            }
        }

        // remove lines from list_destination_files.csv
        let string_local_files = fs::read_to_string(path_list_local_files).unwrap();
        let vec_sorted_local: Vec<&str> = string_local_files.lines().collect();
        // I must create a new vector.
        let mut string_new_local = String::with_capacity(string_local_files.len());
        println!("sorting local list... It will take a minute or two.");
        for line in vec_sorted_local {
            if !list_for_trash.contains(line) {
                string_new_local.push_str(line);
                string_new_local.push_str("\n");
            }
        }
        // save the new local
        unwrap!(fs::write(path_list_local_files, &string_new_local));

        // empty the list if all is successful
        // println!("empty the list if all is successful");
        unwrap!(fs::write(path_list_for_trash, ""));
    }
}





/// Move to trash folder the folders from list_for_trash_folders.
pub fn trash_folders(file_list_for_trash_folders: &mut FileTxt, base_path: &str) {
    let list_for_trash_folders = file_list_for_trash_folders.read_to_string().unwrap();
    if list_for_trash_folders.is_empty() {
        println!("list_for_trash_folders is empty");
    } else {
        let now_string = chrono::Local::now().format("trash_%Y-%m-%d_%H-%M-%S").to_string();
        let base_trash_path = format!("{}_{}", base_path, &now_string);
        if !path::Path::new(&base_trash_path).exists() {
            fs::create_dir_all(&base_trash_path).unwrap();
        }
        for string_path_for_trash in list_for_trash_folders.lines() {
            let move_from = format!("{}{}", base_path, string_path_for_trash);
            let path_move_from = path::Path::new(&move_from);

            // move to trash if file exists. Nothing if it does not exist, maybe is deleted when moved or in a move_to_trash before.
            if path_move_from.exists() {
                let move_to = format!("{}{}", base_trash_path, string_path_for_trash);
                println!("{}  ->  {}", move_from, move_to);
                let parent = unwrap!(path::Path::parent(path::Path::new(&move_to)));
                if !parent.exists() {
                    fs::create_dir_all(&parent).unwrap();
                }
                unwrap!(fs::rename(&move_from, &move_to));
            }
        }
        file_list_for_trash_folders.empty().unwrap();
    }
}
 */
