// local_disk_mod.rs

//! Module contains all functions for local external disk.

#[allow(unused_imports)]
use dropbox_content_hasher::DropboxContentHasher;

// type alias for better expressing coder intention,
// but programmatically identical to the underlying type
type ThreadName=String;

/* use log::error;
use std::io::Write;
use std::path;
use uncased::UncasedStr;
use unwrap::unwrap; */

use crate::{global_config, LibError};

/// the logic is in the LIB project, but all UI is in the CLI project
/// they run on different threads and communicate
/// It uses the global APP_STATE for all config data
pub fn list_local(ui_tx: std::sync::mpsc::Sender<(String,ThreadName)>) -> Result<(), LibError> {
    // empty the file. I want all or nothing result here if the process is terminated prematurely.
    let mut file_list_destination_files = crate::FileTxt::open_for_read_and_write(global_config().path_list_destination_files)?;
    file_list_destination_files.empty()?;
    let mut file_list_destination_folders = crate::FileTxt::open_for_read_and_write(global_config().path_list_destination_folders)?;
    file_list_destination_folders.empty()?;
    let mut file_list_destination_readonly_files = crate::FileTxt::open_for_read_and_write(global_config().path_list_destination_readonly_files)?;
    file_list_destination_readonly_files.empty()?;
    // just_loaded is obsolete once I got the fresh local list
    let mut file_list_just_downloaded_or_moved = crate::FileTxt::open_for_read_and_write(global_config().path_list_just_downloaded_or_moved)?;
    file_list_just_downloaded_or_moved.empty()?;

    // write data to a big string in memory (for my use-case it is >25 MB)
    let mut files_string = String::with_capacity(40_000_000);
    let mut folders_string = String::new();
    let mut readonly_files_string = String::new();
    use walkdir::WalkDir;
    let base_path = std::fs::read_to_string(global_config().path_list_ext_disk_base_path)?;

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
                    ui_tx
                        .send((format!("{file_count}: {}", crate::shorten_string(str_path.trim_start_matches(&base_path), 80)),format!("L0")))
                        .expect("Error mpsc send");

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

    ui_tx.send((format!("local_folder_count: {folder_count}"),"L0".to_string())).expect("Error mpsc send");

    // region: sort
    let files_sorted_string = crate::sort_string_lines(&files_string);
    let folders_sorted_string = crate::sort_string_lines(&folders_string);
    let readonly_files_sorted_string = crate::sort_string_lines(&readonly_files_string);
    // end region: sort
    file_list_destination_files.write_str(&files_sorted_string)?;
    file_list_destination_folders.write_str(&folders_sorted_string)?;
    file_list_destination_readonly_files.write_str(&readonly_files_sorted_string)?;

    ui_tx.send(("All lists stored in files.".to_string(),"L0".to_string())).expect("Error mpsc send");

    Ok(())
}

/// The backup files must not be readonly to allow copying the modified file from the remote.
/// The FileTxt is read+write. It is opened in the bin and not in lib, but it is manipulated only in lib.
pub fn read_only_remove(file_destination_readonly_files: &mut crate::FileTxt, base_path: &str,ui_tx: std::sync::mpsc::Sender<String>)-> Result<(), LibError>  {
    let list_destination_readonly_files = file_destination_readonly_files.read_to_string()?;
    for string_path_for_readonly in list_destination_readonly_files.lines() {
        ui_tx .send(format!("{string_path_for_readonly}")) .expect("Error mpsc send");
        let global_path_to_readonly = format!("{base_path}{string_path_for_readonly}");
        let path_global_path_to_readonly = std::path::Path::new(&global_path_to_readonly);
        // if path does not exist ignore
        if path_global_path_to_readonly.exists() {
            let mut perms = path_global_path_to_readonly.metadata()?.permissions();
            if perms.readonly() == true {
                perms.set_readonly(false);
                std::fs::set_permissions(path_global_path_to_readonly, perms)?;
            }
        }
    }
    file_destination_readonly_files.empty()?;
    Ok(())
}

/*
fn get_content_hash(path_for_download: &str) -> String {
    let token = crate::remote_dropbox_mod::get_short_lived_access_token();
    let client = dropbox_sdk::default_client::UserAuthDefaultClient::new(token);
    unwrap!(crate::remote_dropbox_mod::remote_content_hash(path_for_download, &client))
}

/// Files are often moved or renamed
/// After compare, the same file (with different path or name) will be in the list_for_trash and in the list_for_download.
/// First for every trash line, we search list_for_download for same size and modified.
/// If found, get the remote_metadata with content_hash and calculate local_content_hash.
/// If they are equal move or rename, else nothing: it will be trashed and downloaded eventually.
/// Remove also the lines in files list_for_trash and list_for_download.
pub fn move_or_rename_local_files(app_config: &'static AppConfig) {
    let to_base_local_path = fs::read_to_string(app_config.path_list_ext_disk_base_path).unwrap();
    /*     let token = crate::remote_dropbox_mod::get_short_lived_access_token();
    let client = dropbox_sdk::default_client::UserAuthDefaultClient::new(token); */
    move_or_rename_local_files_internal(
        &to_base_local_path,
        app_config.path_list_for_trash,
        app_config.path_list_for_download,
        app_config.path_list_just_downloaded_or_moved,
    );
}

/// internal function
fn move_or_rename_local_files_internal(to_base_local_path: &str, path_list_for_trash: &str, path_list_for_download: &str, list_just_downloaded_or_moved: &str) {
    let list_for_trash = fs::read_to_string(path_list_for_trash).unwrap();
    let list_for_download = fs::read_to_string(path_list_for_download).unwrap();

    // write the renamed files to list_just_downloaded_or_moved, later they will be added to list_destination_files.csv
    let mut just_downloaded = fs::OpenOptions::new().create(true).append(true).open(list_just_downloaded_or_moved).unwrap();
    let mut count_moved = 0;
    for line_for_trash in list_for_trash.lines() {
        let vec_line_for_trash: Vec<&str> = line_for_trash.split("\t").collect();
        let string_path_for_trash = vec_line_for_trash[0];
        let global_path_to_trash = format!("{}{}", &to_base_local_path, string_path_for_trash);
        let path_global_path_to_trash = path::Path::new(&global_path_to_trash);
        // if path does not exist ignore, probably it eas moved or trashed earlier
        if path_global_path_to_trash.exists() {
            let modified_for_trash = vec_line_for_trash[1];
            let size_for_trash = vec_line_for_trash[2];
            let file_name_for_trash: Vec<&str> = string_path_for_trash.split("/").collect();
            let file_name_for_trash = unwrap!(file_name_for_trash.last());

            // search in list_for_download for possible candidates
            // first try exact match with name, date, size because it is fast
            let mut is_moved = false;
            for line_for_download in list_for_download.lines() {
                let vec_line_for_download: Vec<&str> = line_for_download.split("\t").collect();
                let path_for_download = vec_line_for_download[0];
                let modified_for_download = vec_line_for_download[1];
                let size_for_download = vec_line_for_download[2];
                let file_name_for_download: Vec<&str> = path_for_download.split("/").collect();
                let file_name_for_download = unwrap!(file_name_for_download.last());

                if modified_for_trash == modified_for_download && size_for_trash == size_for_download && file_name_for_trash == file_name_for_download {
                    move_internal(path_global_path_to_trash, &to_base_local_path, path_for_download);
                    unwrap!(writeln!(just_downloaded, "{}", line_for_download));
                    count_moved += 1;
                    is_moved = true;
                    break;
                }
            }
            // if the exact match didn't move the file, then check the content_hash (slow)
            if is_moved == false {
                for line_for_download in list_for_download.lines() {
                    let vec_line_for_download: Vec<&str> = line_for_download.split("\t").collect();
                    let path_for_download = vec_line_for_download[0];
                    let modified_for_download = vec_line_for_download[1];
                    let size_for_download = vec_line_for_download[2];

                    if modified_for_trash == modified_for_download && size_for_trash == size_for_download {
                        // same size and date. Let's check the content_hash to be sure.
                        let local_content_hash = format!("{:x}", unwrap!(DropboxContentHasher::hash_file(path_global_path_to_trash)));
                        let remote_content_hash = get_content_hash(path_for_download);

                        if local_content_hash == remote_content_hash {
                            move_internal(path_global_path_to_trash, &to_base_local_path, path_for_download);
                            unwrap!(writeln!(just_downloaded, "{}", line_for_download));
                            count_moved += 1;
                            break;
                        }
                    }
                }
            }
        }
    }
    println!("moved or renamed: {}", count_moved);
}

/// internal code to move file
fn move_internal(path_global_path_to_trash: &path::Path, to_base_local_path: &str, path_for_download: &str) {
    let move_from = path_global_path_to_trash;
    let move_to = format!("{}{}", to_base_local_path, path_for_download);
    println!("move {}  ->  {}", &move_from.to_string_lossy(), move_to);
    let parent = unwrap!(path::Path::parent(path::Path::new(&move_to)));
    if !parent.exists() {
        fs::create_dir_all(&parent).unwrap();
    }
    if path::Path::new(&move_to).exists() {
        let mut perms = fs::metadata(&move_to).unwrap().permissions();
        if perms.readonly() == true {
            perms.set_readonly(false);
            fs::set_permissions(&move_to, perms).unwrap();
        }
    }
    if path::Path::new(&move_from).exists() {
        let mut perms = unwrap!(fs::metadata(&move_from)).permissions();
        if perms.readonly() == true {
            perms.set_readonly(false);
            fs::set_permissions(&move_from, perms).unwrap();
        }
    }
    unwrap!(fs::rename(&move_from, &move_to));
}

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

/// modify the date od files from list_for_correct_time
pub fn correct_time_from_list(app_config: &'static AppConfig) {
    /*     let token = crate::remote_dropbox_mod::get_short_lived_access_token();
    let client = dropbox_sdk::default_client::UserAuthDefaultClient::new(token); */
    let base_local_path = fs::read_to_string(app_config.path_list_ext_disk_base_path).unwrap();
    correct_time_from_list_internal(&base_local_path, app_config.path_list_for_correct_time);
}

/// modify the date od files from list_for_correct_time
fn correct_time_from_list_internal(base_local_path: &str, path_list_for_correct_time: &str) {
    let mut file_list_for_correct_time = FileTxt::open_for_read_and_write(path_list_for_correct_time).unwrap();
    let list_for_correct_time = file_list_for_correct_time.read_to_string().unwrap();
    for path_to_correct_time in list_for_correct_time.lines() {
        let line: Vec<&str> = path_to_correct_time.split("\t").collect();
        let remote_path = line[0];
        let local_path = format!("{}{}", base_local_path, remote_path);
        if path::Path::new(&local_path).exists() {
            let remote_content_hash = get_content_hash(remote_path);
            let local_content_hash = format!("{:x}", unwrap!(DropboxContentHasher::hash_file(&local_path)));
            if local_content_hash == remote_content_hash {
                let modified = filetime::FileTime::from_system_time(unwrap!(humantime::parse_rfc3339(line[1])));
                unwrap!(filetime::set_file_mtime(local_path, modified));
                // TODO: correct also in list_destination_files.csv, so I can make a new compare after this action
            } else {
                error!("correct_time content_hash different: {}", remote_path);
            }
        }
    }
    // empty the list
    file_list_for_correct_time.empty().unwrap();
}

/// add just downloaded files to list_local (from dropbox remote)
pub fn add_just_downloaded_to_list_local(app_config: &'static AppConfig) {
    let path_list_local_files = app_config.path_list_destination_files;
    add_just_downloaded_to_list_local_internal(app_config.path_list_just_downloaded_or_moved, path_list_local_files);
}

/// add lines from just_downloaded to list_local. Only before compare.
fn add_just_downloaded_to_list_local_internal(path_list_just_downloaded: &str, path_list_local_files: &str) {
    let string_just_downloaded = fs::read_to_string(path_list_just_downloaded).unwrap();
    if !string_just_downloaded.is_empty() {
        // it must be sorted, because downloads are multi-thread and not in sort order
        let string_sorted_just_downloaded = crate::sort_string_lines(&string_just_downloaded);
        let mut vec_sorted_downloaded: Vec<&str> = string_sorted_just_downloaded.lines().collect();
        // It is forbidden to have duplicate lines
        vec_sorted_downloaded.dedup();
        println!("{}: {}", path_list_just_downloaded.split("/").collect::<Vec<&str>>()[1], vec_sorted_downloaded.len());
        unwrap!(fs::write(path_list_just_downloaded, &string_sorted_just_downloaded));

        let string_local_files = fs::read_to_string(path_list_local_files).unwrap();
        let mut vec_sorted_local: Vec<&str> = string_local_files.lines().collect();

        // loop the 2 lists and merge sorted
        let mut cursor_downloaded = 0;
        let mut cursor_local = 0;
        let mut vec_line_local: Vec<&str> = vec![];
        let mut vec_line_downloaded: Vec<&str> = vec![];
        loop {
            vec_line_local.truncate(3);
            vec_line_downloaded.truncate(3);

            if cursor_downloaded >= vec_sorted_downloaded.len() && cursor_local >= vec_sorted_local.len() {
                break;
            } else if cursor_downloaded >= vec_sorted_downloaded.len() {
                // final lines
                break;
            } else if cursor_local >= vec_sorted_local.len() {
                // final lines
                vec_line_downloaded = vec_sorted_downloaded[cursor_downloaded].split("\t").collect();
                vec_sorted_local.push(&vec_sorted_downloaded[cursor_downloaded]);
                cursor_downloaded += 1;
            } else {
                vec_line_downloaded = vec_sorted_downloaded[cursor_downloaded].split("\t").collect();
                vec_line_local = vec_sorted_local[cursor_local].split("\t").collect();
                // UncasedStr preserves the case in the string, but comparison is done case insensitive
                let path_downloaded: &UncasedStr = vec_line_downloaded[0].into();
                let path_local: &UncasedStr = vec_line_local[0].into();
                if path_downloaded.lt(path_local) {
                    // insert the line
                    vec_sorted_local.insert(cursor_local, vec_sorted_downloaded[cursor_downloaded]);
                    cursor_local += 1;
                    cursor_downloaded += 1;
                } else if path_downloaded.gt(path_local) {
                    cursor_local += 1;
                } else {
                    // equal path. replace line
                    vec_sorted_local[cursor_local] = vec_sorted_downloaded[cursor_downloaded];
                    cursor_local += 1;
                    cursor_downloaded += 1;
                }
            }
        }

        let new_local_files = vec_sorted_local.join("\n");
        unwrap!(fs::write(path_list_local_files, &new_local_files));

        // empty the file temp_data/list_just_downloaded_or_moved.csv
        // println!("list_just_downloaded_or_moved emptied");
        unwrap!(fs::write(path_list_just_downloaded, ""));
    }
}

/// create new empty folders
pub fn create_folders(file_list_for_create_folders: &mut FileTxt, base_path: &str) {
    let list_for_create_folders = file_list_for_create_folders.read_to_string().unwrap();
    if list_for_create_folders.is_empty() {
        println!("list_for_create_folders is empty");
    } else {
        for string_path in list_for_create_folders.lines() {
            let global_path = format!("{}{}", base_path, string_path);
            let path_global_path = path::Path::new(&global_path);
            // if path exists ignore
            if !path_global_path.exists() {
                dbg!(path_global_path);
                fs::create_dir_all(path_global_path).unwrap();
            }
        }
        file_list_for_create_folders.empty().unwrap();
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
