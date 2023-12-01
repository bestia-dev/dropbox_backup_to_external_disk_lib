// remote_dropbox_mod.rs

//! Module contains all the communication with the remote dropbox storage.
//! It uses inter-thread channels to send text to the UserInterface thread.
//! It uses LibError to return error text to the UI thread.

use crate::app_state_mod::APP_STATE;
use crate::error_mod::LibError;
use crate::utils_mod::println_to_ui_thread_with_thread_name;
use crate::FileTxt;

// type alias for better expressing coder intention,
// but programmatically identical to the underlying type
type FolderList = Vec<String>;
type FileList = Vec<String>;
type FolderListAndFileList = (Vec<String>, Vec<String>);
type ThreadNum = i32;
type ThreadName = String;
type MasterKey = String;
type TokenEnc = String;

/// This is a short-lived token, so security is not my primary concern.
/// But it is bad practice to store anything as plain text. I will encode it and store it in env var.
/// This is more like an obfuscation tactic to make it harder, but in no way impossible, to find out the secret.
pub fn encode_token(token: String) -> Result<(MasterKey, TokenEnc), LibError> {
    // every time, the master key will be random and temporary
    let master_key = fernet::Fernet::generate_key();
    let fernet = fernet::Fernet::new(&master_key).ok_or_else(|| LibError::ErrorFromStr("Error Fernet key is not correct."))?;
    let token_enc = fernet.encrypt(token.as_bytes());
    Ok((master_key, token_enc))
}

/// test authentication with dropbox.com
/// experiment with sending function pointer
pub fn test_connection() -> Result<(), LibError> {
    let token = get_authorization_token()?;
    let client = dropbox_sdk::default_client::UserAuthDefaultClient::new(token);
    (dropbox_sdk::files::list_folder(&client, &dropbox_sdk::files::ListFolderArg::new("".to_string()))?)?;
    Ok(())
}

/// read encoded token (from env), decode and return the authorization token
pub fn get_authorization_token() -> Result<dropbox_sdk::oauth2::Authorization, LibError> {
    // the global APP_STATE method reads encoded tokens from env var
    let (master_key, token_enc) = APP_STATE.get().expect("Bug: OnceCell").load_keys_from_io()?;
    let fernet = fernet::Fernet::new(&master_key).ok_or_else(|| LibError::ErrorFromStr("Error Fernet master key is not correct."))?;
    let token = fernet.decrypt(&token_enc)?;
    let token = String::from_utf8(token)?;
    // return
    Ok(dropbox_sdk::oauth2::Authorization::from_access_token(token))
}

#[allow(unused_imports)]
use std::collections::VecDeque;
use std::path::Path;
use std::sync::mpsc;

/// get remote list in parallel
/// first get the first level of folders and then request in parallel sub-folders recursively
pub fn list_remote(ui_tx: std::sync::mpsc::Sender<(String, ThreadName)>, mut file_list_source_files: FileTxt, mut file_list_source_folders: FileTxt) -> Result<(), LibError> {
    // empty the files. I want all or nothing result here if the process is terminated prematurely.
    file_list_source_files.empty()?;
    file_list_source_folders.empty()?;

    let token = get_authorization_token()?;
    let client = dropbox_sdk::default_client::UserAuthDefaultClient::new(token.clone());
    let client_ref = &client;
    // walkdir non-recursive for the first level of folders
    let (folder_list_root, file_list_root) = list_remote_folder(&client, "/", 0, false, ui_tx.clone())?;

    // threadpool with 8 threads
    let pool = rayon::ThreadPoolBuilder::new().num_threads(8).build().expect("Bug: rayon ThreadPoolBuilder");
    pool.scope({
        // Prepare variables to be moved/captured to the closure. All is isolated in a block scope.
        let mut folder_list_all = vec![];
        let mut file_list_all = file_list_root;
        // channel for inter-thread communication to send folder/files lists
        let (list_tx, list_rx) = mpsc::channel();
        // only the closure is actually spawned, because it is the return value of the block
        move |scoped| {
            // these folders will request walkdir recursive in parallel
            // loop in a new thread, so the send msg will come immediately
            for folder_path in folder_list_root.iter() {
                // execute in a separate threads, or waits for a free thread from the pool
                // scoped.spawn closure cannot return a Result<>, but it can send it as inter-thread message
                scoped.spawn({
                    // Prepare variables to be moved/captured to the closure. All is isolated in a block scope.
                    // We are in a loop here. It means for every round we need new clones to move/capture/consume.
                    let folder_path = folder_path.clone();
                    let ui_tx_move_to_closure = ui_tx.clone();
                    let ui_tx_move_to_closure_2 = ui_tx.clone();
                    let list_tx_move_to_closure = list_tx.clone();
                    // only the closure is actually spawned, because it is the return value of the block
                    move |_| {
                        let thread_num = rayon::current_thread_index().expect("Bug: rayon current_thread_index") as ThreadNum;
                        // catch propagated errors and communicate errors to user or developer
                        // spawned closure cannot propagate error with ?
                        match list_remote_folder(client_ref, &folder_path, thread_num, true, ui_tx_move_to_closure) {
                            Ok(folder_list_and_file_list) => list_tx_move_to_closure.send(folder_list_and_file_list).expect("Bug: mpsc send"),
                            Err(err) => println_to_ui_thread_with_thread_name(&ui_tx_move_to_closure_2, format!("Error in thread {err}"), &format!("R{thread_num}")),
                        }
                    }
                });
            }

            // the receiver reads all msgs from the queue
            drop(list_tx);
            let mut all_folder_count = 0;
            let mut all_file_count = 0;
            for (folder_list, file_list) in &list_rx {
                all_folder_count += folder_list.len();
                all_file_count += file_list.len();
                folder_list_all.extend_from_slice(&folder_list);
                file_list_all.extend_from_slice(&file_list);
            }

            println_to_ui_thread_with_thread_name(&ui_tx, format!("remote list file sort {all_file_count}"), "R0");
            let string_file_list = crate::utils_mod::sort_list(file_list_all);
            file_list_source_files.write_append_str(&string_file_list).expect("Bug: file_list_source_files must be writable");

            println_to_ui_thread_with_thread_name(&ui_tx, format!("remote list folder sort: {all_folder_count}"), "R0");
            let string_folder_list = crate::utils_mod::sort_list(folder_list_all);
            file_list_source_folders.write_append_str(&string_folder_list).expect("Bug: file_list_source_folders must be writable");
        }
    });

    Ok(())
}

/// list remote folder
pub fn list_remote_folder(
    client: &dropbox_sdk::default_client::UserAuthDefaultClient,
    path: &str,
    thread_num: ThreadNum,
    recursive: bool,
    ui_tx: mpsc::Sender<(String, ThreadName)>,
) -> Result<FolderListAndFileList, LibError> {
    let mut folder_list: FolderList = vec![];
    let mut file_list: FileList = vec![];
    let mut last_send_ms = std::time::Instant::now();

    match dropbox_list_folder(client, path, recursive) {
        Ok(Ok(iterator)) => {
            for entry_result in iterator {
                match entry_result {
                    Ok(Ok(dropbox_sdk::files::Metadata::Folder(entry))) => {
                        // path_display is not 100% case accurate. Dropbox is case-insensitive and preserves the casing only for the metadata_name, not path.
                        let folder_path = entry.path_display.unwrap_or(entry.name);
                        // writing to screen is slow, I will not write every folder/file, but will wait for 100ms
                        if last_send_ms.elapsed().as_millis() >= 100 {
                            println_to_ui_thread_with_thread_name(&ui_tx, format!("Folder: {}", crate::shorten_string(&folder_path, 80)), &format!("R{thread_num}"));
                            last_send_ms = std::time::Instant::now();
                        }
                        folder_list.push(folder_path);
                    }
                    Ok(Ok(dropbox_sdk::files::Metadata::File(entry))) => {
                        // write csv tab delimited
                        // avoid strange files *com.dropbox.attrs
                        // path_display is not 100% case accurate. Dropbox is case-insensitive and preserves the casing only for the metadata_name, not path.
                        let file_path = entry.path_display.unwrap_or(entry.name);
                        if !file_path.ends_with("com.dropbox.attrs") {
                            // writing to screen is slow, I will not write every folder/file, but will wait for 100ms
                            if last_send_ms.elapsed().as_millis() >= 100 {
                                println_to_ui_thread_with_thread_name(&ui_tx, format!("File: {}", crate::shorten_string(&file_path, 80)), &format!("R{thread_num}"));
                                last_send_ms = std::time::Instant::now();
                            }
                            file_list.push(format!("{}\t{}\t{}", file_path, entry.client_modified, entry.size));
                        }
                    }
                    Ok(Ok(dropbox_sdk::files::Metadata::Deleted(_entry))) => {
                        return Err(LibError::ErrorFromString(format!("R{thread_num} Error unexpected deleted entry")));
                    }
                    Ok(Err(e)) => {
                        return Err(LibError::ErrorFromString(format!("R{thread_num} Error from files/list_folder_continue: {e}")));
                    }
                    Err(e) => {
                        return Err(LibError::ErrorFromString(format!("R{thread_num} Error API request: {e}")));
                    }
                }
            }
            // return FolderListAndFileList
            Ok((folder_list, file_list))
        }
        Ok(Err(e)) => Err(LibError::ErrorFromString(format!("R{thread_num} Error from files/list_folder: {e}"))),
        Err(e) => Err(LibError::ErrorFromString(format!("R{thread_num} Error API request: {e}"))),
    }
}

/// dropbox function to list folders
fn dropbox_list_folder<'a>(
    client: &'a dropbox_sdk::default_client::UserAuthDefaultClient,
    path: &str,
    recursive: bool,
) -> dropbox_sdk::Result<Result<DirectoryIterator<'a>, dropbox_sdk::files::ListFolderError>> {
    // validate input parameters
    // assert! macro will panic if false.
    // Is is not used for user input validation but to express an assumption (for code readers) and to catch coding bugs (in the caller).
    // You express which invariant should always be true and therefore there should be no failure. If a failure happens it is a coding bug in the caller.
    // An invariant is any "logical rule that must be obeyed" (assumption) that can be communicated to a human, but not to your compiler.
    assert!(path.starts_with('/'), "Error path needs to be absolute (start with a '/')");

    let path = if path == "/" {
        // Root folder should be requested as empty string
        String::new()
    } else {
        path.to_owned()
    };

    match dropbox_sdk::files::list_folder(client, &dropbox_sdk::files::ListFolderArg::new(path).with_recursive(recursive)) {
        Ok(Ok(result)) => {
            let cursor = if result.has_more { Some(result.cursor) } else { None };

            Ok(Ok(DirectoryIterator {
                client,
                cursor,
                buffer: result.entries.into(),
            }))
        }
        Ok(Err(e)) => Ok(Err(e)),
        Err(e) => Err(e),
    }
}

/// iterator for Directory on remote Dropbox storage
struct DirectoryIterator<'a> {
    client: &'a dropbox_sdk::default_client::UserAuthDefaultClient,
    buffer: VecDeque<dropbox_sdk::files::Metadata>,
    cursor: Option<String>,
}

impl<'a> Iterator for DirectoryIterator<'a> {
    type Item = dropbox_sdk::Result<Result<dropbox_sdk::files::Metadata, dropbox_sdk::files::ListFolderContinueError>>;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(entry) = self.buffer.pop_front() {
            Some(Ok(Ok(entry)))
        } else if let Some(cursor) = self.cursor.take() {
            match dropbox_sdk::files::list_folder_continue(self.client, &dropbox_sdk::files::ListFolderContinueArg::new(cursor)) {
                Ok(Ok(result)) => {
                    self.buffer.extend(result.entries);
                    if result.has_more {
                        self.cursor = Some(result.cursor);
                    }
                    self.buffer.pop_front().map(|entry| Ok(Ok(entry)))
                }
                Ok(Err(e)) => Some(Ok(Err(e))),
                Err(e) => Some(Err(e)),
            }
        } else {
            None
        }
    }
}

/// get content_hash from remote
pub fn remote_content_hash(remote_path: &str, client: &dropbox_sdk::default_client::UserAuthDefaultClient) -> Option<String> {
    let arg = dropbox_sdk::files::GetMetadataArg::new(remote_path.to_string());
    let res_res_metadata = dropbox_sdk::files::get_metadata(client, &arg);

    match res_res_metadata {
        Ok(Ok(dropbox_sdk::files::Metadata::Folder(_entry))) => {
            return None;
        }
        Ok(Ok(dropbox_sdk::files::Metadata::File(entry))) => {
            return Some(entry.content_hash.expect("Bug: dropbox metadata must have hash."));
        }
        Ok(Ok(dropbox_sdk::files::Metadata::Deleted(_entry))) => {
            return None;
        }
        Ok(Err(e)) => {
            println!("Error get metadata: {}", e);
            return None;
        }
        Err(e) => {
            println!("API request error: {}", e);
            return None;
        }
    }
}

/// download one file is calling internally download_from_vec
/// This is used just for debugging. For real the user will run download_from_list.
pub fn download_one_file(
    ui_tx: std::sync::mpsc::Sender<(String, ThreadName)>,
    ext_disk_base_path: &Path,
    path_to_download: &Path,
    file_list_just_downloaded: &mut FileTxt,
    file_powershell_script_change_modified_datetime: &mut FileTxt,
) -> Result<(), LibError> {
    let path_str = path_to_download.to_string_lossy().to_string();
    let mut vec_list_for_download: Vec<&str> = vec![&path_str];
    download_from_vec(
        ui_tx,
        ext_disk_base_path,
        &mut vec_list_for_download,
        file_list_just_downloaded,
        file_powershell_script_change_modified_datetime,
    )?;

    Ok(())
}

/// download files from list
/// It removes just_downloaded from list_for_download, so this function can be stopped and then called again.
pub fn download_from_list(
    ui_tx: std::sync::mpsc::Sender<(String, ThreadName)>,
    ext_disk_base_path: &Path,
    file_list_for_download: &mut FileTxt,
    file_list_just_downloaded: &mut FileTxt,
    file_powershell_script_change_modified_datetime: &mut FileTxt,
) -> Result<(), LibError> {
    let list_for_download = file_list_for_download.read_to_string()?;
    let mut vec_list_for_download: Vec<&str> = list_for_download.lines().collect();

    //remove list_just_downloaded from list_for_download
    let list_just_downloaded = file_list_just_downloaded.read_to_string()?;
    if !list_just_downloaded.is_empty() {
        let vec_list_just_downloaded: Vec<&str> = list_just_downloaded.lines().collect();
        for just_downloaded in vec_list_just_downloaded.iter() {
            vec_list_for_download.retain(|line| !line.starts_with(just_downloaded))
        }
        let string_for_download = vec_list_for_download.join("\n");
        file_list_for_download.empty()?;
        file_list_for_download.write_append_str(&string_for_download)?;
        file_list_just_downloaded.empty()?;
    }

    download_from_vec(
        ui_tx,
        ext_disk_base_path,
        &mut vec_list_for_download,
        file_list_just_downloaded,
        file_powershell_script_change_modified_datetime,
    )?;

    Ok(())
}

fn download_from_vec(
    ui_tx: std::sync::mpsc::Sender<(String, ThreadName)>,
    ext_disk_base_path: &Path,
    vec_list_for_download: &mut Vec<&str>,
    file_list_just_downloaded: &mut FileTxt,
    file_powershell_script_change_modified_datetime: &mut FileTxt,
) -> Result<(), LibError> {
    println_to_ui_thread_with_thread_name(
        &ui_tx,
        format!(
            r#"
If you are running this program on WSL Debian then it cannot change 
the modified datetime of the files on an external disk with exFAT. I don't know why! :-(
The workaround is to run manually the generated powershell script temp_data/powershell_script_change_modified_datetime.ps
"#
        ),
        "Warning",
    );

    let token = get_authorization_token()?;
    let client = dropbox_sdk::default_client::UserAuthDefaultClient::new(token);
    // I have to create a reference before the move-closure. So the reference is moved to the closure and not the object.
    let client_ref = &client;
    // channel for inter-thread communication to send messages that will be appended to files
    let (files_append_tx, files_append_rx) = mpsc::channel();
    //8 threads to download in parallel
    let pool = rayon::ThreadPoolBuilder::new().num_threads(8).build().unwrap();
    pool.scope(move |scoped| {
        for line_path_to_download in vec_list_for_download.iter() {
            // execute in 4 separate threads, or waits for a free thread from the pool
            scoped.spawn({
                // Prepare variables to be moved/captured to the closure. All is isolated in a block scope.
                let line: Vec<&str> = line_path_to_download.split("\t").collect();
                let path_to_download = line[0];
                let ui_tx_clone = ui_tx.clone();
                let ui_tx_move_to_closure_2 = ui_tx.clone();
                let files_append_tx_move_to_closure = files_append_tx.clone();
                // only the closure is actually spawned, because it is the return value of the block
                move |_| {
                    let thread_num = rayon::current_thread_index().expect("Bug: thread num must exist.");
                    // catch propagated errors and communicate errors to user or developer
                    // spawned closure cannot propagate error with ?
                    match download_internal(
                        ui_tx_clone,
                        ext_disk_base_path,
                        client_ref,
                        thread_num as i32,
                        Path::new(path_to_download),
                        files_append_tx_move_to_closure,
                    ) {
                        Ok(()) => {},
                        Err(err) => println_to_ui_thread_with_thread_name(&ui_tx_move_to_closure_2, format!("Error in thread {err}"), &format!("R{thread_num}")),
                    }
                }
            });
        }

        // the receiver reads all msgs from the queue
        // and them appends it neatly in files. Because only this thread writes to files there cannot be data race condition.
        drop(files_append_tx);
        for (just_downloaded, ps_command) in &files_append_rx {
            if !just_downloaded.is_empty() {
                file_list_just_downloaded
                    .write_append_str(&format!("{just_downloaded}\n"))
                    .expect("Bug: file_list_just_downloaded must be writable.");
            }
            if !ps_command.is_empty() {
                file_powershell_script_change_modified_datetime
                    .write_append_str(&format!("{ps_command}\n"))
                    .expect("Bug: file_powershell_script_change_modified_datetime must be writable.");
            }
        }
    });

    Ok(())
}

/// download one file with client object dropbox_sdk::default_client::UserAuthDefaultClient
fn download_internal(
    ui_tx: mpsc::Sender<(String, ThreadName)>,
    ext_disk_base_path: &Path,
    client: &dropbox_sdk::default_client::UserAuthDefaultClient,
    thread_num: i32,
    path_to_download: &Path,
    files_append_tx: mpsc::Sender<(String, String)>,
) -> Result<(), LibError> {
    let thread_name = format!("R{thread_num}");
    let local_path = ext_disk_base_path.join(path_to_download.to_string_lossy().trim_start_matches("/"));
    // create folder if it does not exist
    let parent = local_path.parent().expect("Bug: Parent must exist.");
    if !parent.exists() {
        std::fs::create_dir_all(parent)?;
    }
    let file_name = path_to_download.file_name().expect("Bug: Filename must exist.");

    let modified_str;
    let metadata_size;
    let mut just_downloaded = String::new();

    // get datetime from remote
    let get_metadata_arg = dropbox_sdk::files::GetMetadataArg::new(path_to_download.to_string_lossy().to_string());
    let metadata = (dropbox_sdk::files::get_metadata(client, &get_metadata_arg)?)?;
    match metadata {
        dropbox_sdk::files::Metadata::File(metadata) => {
            modified_str = metadata.client_modified;
            metadata_size = metadata.size;
        }
        _ => {
            return Err(LibError::ErrorFromStr("This is not a file on Dropbox"));
        }
    }
    let system_time = humantime::parse_rfc3339(&modified_str).expect("Bug: parse_rfc3339 must succeed");
    let modified = filetime::FileTime::from_system_time(system_time);

    // files of size 0 cannot be downloaded. I will just create them empty, because download empty file causes error 416
    if metadata_size == 0 {
        if local_path.exists() {
            std::fs::remove_file(&local_path).expect("Bug: remove file must succeed");
        }
        let _file = FileTxt::open_for_read_and_write(&local_path).expect("Bug: open_for_read_and_write must succeed.");
        println_to_ui_thread_with_thread_name(&ui_tx, local_path.to_string_lossy().to_string(), &thread_name);
    } else {
        let mut bytes_out = 0u64;
        let download_arg = dropbox_sdk::files::DownloadArg::new(path_to_download.to_string_lossy().to_string());
        let base_temp_path_to_download = Path::new("temp_data/temp_download");
        if !base_temp_path_to_download.exists() {
            std::fs::create_dir_all(&base_temp_path_to_download)?;
        }
        let temp_local_path = base_temp_path_to_download.join(file_name);
        let mut file = std::fs::OpenOptions::new().create(true).write(true).open(&temp_local_path)?;
        // I will download to a temp folder and then move the file to the right folder only when the download is complete.
        'download: loop {
            // TODO: I want to press a key to stop the downloading gracefully
            // but this thread is NOT the ui thread
            let result = dropbox_sdk::files::download(client, &download_arg, Some(bytes_out), None);
            match result {
                Ok(Ok(download_result)) => {
                    let mut body = download_result.body.expect("Bug: body must exist");
                    loop {
                        // limit read to 1 MiB per loop iteration so we can output progress
                        // let mut input_chunk = (&mut body).take(1_048_576);
                        use std::io::Read;
                        let mut input_chunk = (&mut body).take(1_048_576);
                        match std::io::copy(&mut input_chunk, &mut file) {
                            Ok(0) => {
                                break 'download;
                            }
                            Ok(len) => {
                                bytes_out += len as u64;
                                if let Some(total) = download_result.content_length {
                                    let string_to_print = format!(
                                        "{:.01}% of {:.02} MB downloading {}",
                                        bytes_out as f64 / total as f64 * 100.,
                                        total as f64 / 1000000.,
                                        crate::shorten_string(&path_to_download.to_string_lossy(), 80)
                                    );
                                    println_to_ui_thread_with_thread_name(&ui_tx, string_to_print, &thread_name);
                                    just_downloaded = path_to_download.to_string_lossy().to_string();
                                } else {
                                    let string_to_print = format!("{} MB downloaded {}", bytes_out as f64 / 1000000., crate::shorten_string(&path_to_download.to_string_lossy(), 80));
                                    println_to_ui_thread_with_thread_name(&ui_tx, string_to_print, &thread_name);
                                }
                            }
                            Err(e) => {
                                let string_to_print = format!("Read error: {}", e);
                                println_to_ui_thread_with_thread_name(&ui_tx, string_to_print, &thread_name);
                                continue 'download; // do another request and resume
                            }
                        }
                    }
                }
                Ok(Err(download_error)) => {
                    let string_to_print = format!("Download error: {}", download_error);
                    println_to_ui_thread_with_thread_name(&ui_tx, string_to_print, &thread_name);
                }
                Err(request_error) => {
                    let string_to_print = format!("Error: {}", request_error);
                    println_to_ui_thread_with_thread_name(&ui_tx, string_to_print, &thread_name);
                }
            }
            break 'download;
        }
        // move the completed download file to his final folder
        // the classic std::fs::rename returns IoError: Invalid cross-device link (os error 18), because it cannot
        // move files from container to mounts in other operating systems.
        // I have to use std::io::copy with code and then delete.
        {
            let mut reader = std::fs::File::open(&temp_local_path)?;
            let mut writer = std::fs::File::create(&local_path)?;
            std::io::copy(&mut reader, &mut writer)?;
            std::fs::remove_file(&temp_local_path)?;
        }
    }
    // cannot change the LastWrite/modified time from the container in WSL to external exFAT on Windows
    // I will instead write a Powershell script to run manually after the program.
    // From this thread I have to send a message to the other thread to avoid multiple threads writing to the same file. That is a no-no.
    let mut ps_command = String::new();
    match filetime::set_file_times(&local_path, modified, modified) {
        Ok(()) => (),
        Err(_) => {
            if local_path.to_string_lossy().starts_with("/mnt/") {
                let win_path = local_path.to_string_lossy().trim_start_matches("/mnt/").to_string();
                // replace only the first / with :/
                let win_path = win_path.replacen("/", ":/", 1);
                // replace all / with \
                let win_path = win_path.replace("/", r#"\"#);
                ps_command = format!(r#"Set-ItemProperty -Path "{win_path}" -Name LastWriteTime -Value {}"#, modified_str);
            }
        }
    }
    // I use the channel files_append_tx to send messages from many threads to just one receiver.
    // That receiver can append to files without worrying of other threads interfering.
    // So only a single thread can write to a file. That is then sure to be serial and never in simultaneously (data race).
    // I send 2 strings for 2 files: file_list_just_downloaded and file_powershell_script_change_modified_datetime
    // Both of them can be empty. That will not be appended to the file.

    files_append_tx.send((just_downloaded, ps_command)).expect("Bug: mpsc send");

    Ok(())
}
