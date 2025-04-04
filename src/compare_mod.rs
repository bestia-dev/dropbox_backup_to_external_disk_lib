// compare_mod.rs

use std::path::Path;

use crate::{utils_mod::println_to_ui_thread, FileTxt, LibError};
use chrono::{DateTime, Utc};
use uncased::UncasedStr;

/// compare list: the lists and produce list_for_download, list_for_trash_files
pub fn compare_files(ui_tx: std::sync::mpsc::Sender<String>, app_config: &'static crate::AppConfig) -> Result<(), LibError> {
    //add_just_downloaded_to_list_local(app_config);
    compare_lists_internal(
        ui_tx,
        app_config.path_list_source_files,
        app_config.path_list_destination_files,
        app_config.path_list_for_download,
        app_config.path_list_for_trash_files,
    )?;
    Ok(())
}

/// compare list: the lists must be already sorted for this to work correctly
fn compare_lists_internal(
    ui_tx: std::sync::mpsc::Sender<String>,
    path_list_source_files: &Path,
    path_list_destination_files: &Path,
    path_list_for_download: &Path,
    path_list_for_trash: &Path,
) -> Result<(), LibError> {
    let file_list_source_files = FileTxt::open_for_read(path_list_source_files)?;
    let string_list_source_files = file_list_source_files.read_to_string()?;
    let vec_list_source_files: Vec<&str> = string_list_source_files.lines().collect();
    println_to_ui_thread(&ui_tx, format!("{}: {}", file_list_source_files.file_name(), vec_list_source_files.len()));

    let file_list_destination_files = FileTxt::open_for_read(path_list_destination_files)?;
    let string_list_destination_files = file_list_destination_files.read_to_string()?;
    let vec_list_destination_files: Vec<&str> = string_list_destination_files.lines().collect();
    println_to_ui_thread(&ui_tx, format!("{}: {}", file_list_destination_files.file_name(), vec_list_destination_files.len()));

    let mut vec_for_download: Vec<String> = vec![];
    let mut vec_for_trash: Vec<String> = vec![];
    let mut cursor_source = 0;
    let mut cursor_destination = 0;
    //avoid making new allocations or shadowing inside a loop
    let mut vec_line_destination: Vec<&str> = vec![];
    let mut vec_line_source: Vec<&str> = vec![];
    //let mut i = 0;
    loop {
        vec_line_destination.truncate(3);
        vec_line_source.truncate(3);

        if cursor_source >= vec_list_source_files.len() && cursor_destination >= vec_list_destination_files.len() {
            break;
        } else if cursor_source >= vec_list_source_files.len() {
            // final lines
            vec_for_trash.push(vec_list_destination_files[cursor_destination].to_string());
            cursor_destination += 1;
        } else if cursor_destination >= vec_list_destination_files.len() {
            // final lines
            vec_for_download.push(vec_list_source_files[cursor_source].to_string());
            cursor_source += 1;
        } else {
            //compare the 2 lines
            // /Video_Backup/DVDs/BikeManual/om/FOXHelp/jap/float_x.htm	2007-01-08T19:31:44Z	45889
            vec_line_source = vec_list_source_files[cursor_source].split("\t").collect();
            vec_line_destination = vec_list_destination_files[cursor_destination].split("\t").collect();
            // UncasedStr preserves the case in the string, but comparison is done case insensitive
            let path_source: &UncasedStr = vec_line_source[0].into();
            let path_destination: &UncasedStr = vec_line_destination[0].into();

            if path_source.lt(path_destination) {
                vec_for_download.push(vec_list_source_files[cursor_source].to_string());
                cursor_source += 1;
            } else if path_source.gt(path_destination) {
                vec_for_trash.push(vec_list_destination_files[cursor_destination].to_string());
                cursor_destination += 1;
            } else {
                // equal names! check date and size
                // incredible, incredible, incredible. exFAT is a Microsoft disk format for external disks. It allows for 10ms resolution for LastWrite/modified datetime.
                // But Microsoft in Win10 driver for exFAT uses only 2seconds resolution. Crazy! After 20 years of existence.
                // this means that if the time difference is less then 2 seconds, they are probably the same file
                let source_modified_dt_utc: DateTime<Utc> = DateTime::parse_from_rfc3339(vec_line_source[1]).expect("Bug: datetime must be correct").into();
                let destination_modified_dt_utc: DateTime<Utc> = DateTime::parse_from_rfc3339(vec_line_destination[1]).expect("Bug: datetime must be correct").into();
                if vec_line_source[2] != vec_line_destination[2] || chrono::Duration::from(source_modified_dt_utc - destination_modified_dt_utc).abs() > chrono::Duration::seconds(2) {
                    vec_for_download.push(vec_list_source_files[cursor_source].to_string());
                }
                // else the metadata is the same, no action
                cursor_destination += 1;
                cursor_source += 1;
            }
        }
    }
    let mut file_list_for_downloads = FileTxt::open_for_read_and_write(path_list_for_download)?;
    println_to_ui_thread(&ui_tx, format!("{}: {}", file_list_for_downloads.file_name(), vec_for_download.len()));
    let string_for_download = vec_for_download.join("\n");
    file_list_for_downloads.write_append_str(&string_for_download)?;

    let mut file_list_for_trash_files = FileTxt::open_for_read_and_write(path_list_for_trash)?;
    println_to_ui_thread(&ui_tx, format!("{}: {}", file_list_for_trash_files.file_name(), vec_for_trash.len()));
    let string_for_trash_files = vec_for_trash.join("\n");
    file_list_for_trash_files.write_append_str(&string_for_trash_files)?;

    Ok(())
}

/// compare folders and write folders to trash into path_list_for_trash_folders
/// the list is already sorted
pub fn compare_folders(
    ui_tx: std::sync::mpsc::Sender<String>,
    string_list_source_folders: &str,
    string_list_destination_folders: &str,
    file_list_for_trash_folders: &mut FileTxt,
    file_list_for_create_folders: &mut FileTxt,
) -> Result<(), LibError> {
    let vec_list_source_folders: Vec<&str> = string_list_source_folders.lines().collect();
    let vec_list_destination_folders: Vec<&str> = string_list_destination_folders.lines().collect();

    let mut vec_for_trash: Vec<String> = vec![];
    file_list_for_trash_folders.empty()?;
    let mut vec_for_create: Vec<String> = vec![];
    file_list_for_create_folders.empty()?;
    let mut cursor_source = 0;
    let mut cursor_destination = 0;

    loop {
        if cursor_source >= vec_list_source_folders.len() && cursor_destination >= vec_list_destination_folders.len() {
            // all lines are processed
            break;
        } else if cursor_destination >= vec_list_destination_folders.len() {
            // final lines
            vec_for_create.push(vec_list_source_folders[cursor_source].to_string());
            cursor_source += 1;
        } else if cursor_source >= vec_list_source_folders.len() {
            // final lines
            vec_for_trash.push(vec_list_destination_folders[cursor_destination].to_string());
            cursor_destination += 1;
        } else {
            // compare the 2 lines
            // UncasedStr preserves the case in the string, but comparison is done case insensitive
            let path_source: &UncasedStr = vec_list_source_folders[cursor_source].into();
            let path_destination: &UncasedStr = vec_list_destination_folders[cursor_destination].into();
            if path_source.lt(path_destination) {
                vec_for_create.push(vec_list_source_folders[cursor_source].to_string());
                cursor_source += 1;
            } else if path_source.gt(path_destination) {
                vec_for_trash.push(vec_list_destination_folders[cursor_destination].to_string());
                cursor_destination += 1;
            } else {
                // else no action, just increment cursors
                cursor_destination += 1;
                cursor_source += 1;
            }
        }
    }
    println_to_ui_thread(&ui_tx, format!("{}: {}", file_list_for_trash_folders.file_name(), vec_for_trash.len()));
    let string_for_trash_files = vec_for_trash.join("\n");
    file_list_for_trash_folders.write_append_str(&string_for_trash_files)?;
    println_to_ui_thread(&ui_tx, format!("{}: {}", file_list_for_create_folders.file_name(), vec_for_create.len()));
    let string_for_create = vec_for_create.join("\n");
    file_list_for_create_folders.write_append_str(&string_for_create)?;
    Ok(())
}

/*
/// add just downloaded files to list_local (from dropbox remote)
pub fn add_just_downloaded_to_list_local(app_config: &'static AppConfig) {
    let path_list_local_files = app_config.path_list_destination_files;
    add_just_downloaded_to_list_local_internal(app_config.path_list_just_downloaded, path_list_local_files);
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

        // empty the file tmp/temp_data/list_just_downloaded.csv
        // println!("list_just_downloaded emptied");
        unwrap!(fs::write(path_list_just_downloaded, ""));
    }
}
 */
