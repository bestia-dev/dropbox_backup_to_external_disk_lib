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
    let token_clone_1 = token.to_owned().clone();
    let client = dropbox_sdk::default_client::UserAuthDefaultClient::new(token_clone_1);

    // channel for inter-thread communication to send folder/files lists
    let (list_tx, list_rx) = mpsc::channel();
    let list_tx_move_to_closure = list_tx.clone();
    // walkdir non-recursive for the first level of folders
    let (folder_list_root, file_list_root) = list_remote_folder(&client, "/", 0, false, ui_tx.clone())?;

    let mut folder_list_all = vec![];
    let mut file_list_all = file_list_root;
    let ui_tx_move_to_closure = ui_tx.clone();

    // these folders will request walkdir recursive in parallel
    // loop in a new thread, so the send msg will come immediately
    let _sender_thread = std::thread::spawn(move || {
        let ui_tx_2 = ui_tx_move_to_closure.clone();
        let list_tx_2 = list_tx_move_to_closure.clone();
        // threadpool with 3 threads
        let pool = rayon::ThreadPoolBuilder::new().num_threads(3).build().expect("Bug: rayon ThreadPoolBuilder");
        pool.scope(|scoped| {
            for folder_path in &folder_list_root {
                // these variables will be moved/captured into the closure
                let folder_path = folder_path.clone();
                let token_clone2 = token.to_owned().clone();
                let ui_tx_3 = ui_tx_2.clone();
                let list_tx_3 = list_tx_2.clone();
                // execute in a separate threads, or waits for a free thread from the pool
                // scoped.spawn closure cannot return a Result<>, but it can send it as inter-thread message
                scoped.spawn(move |_s| {
                    let client = dropbox_sdk::default_client::UserAuthDefaultClient::new(token_clone2.to_owned());
                    // recursive walkdir
                    let thread_num = rayon::current_thread_index().expect("Bug: rayon current_thread_index") as ThreadNum;

                    list_tx_3.send(list_remote_folder(&client, &folder_path, thread_num, true, ui_tx_3)).expect("Bug: mpsc send");
                    // TODO: this send raises error? Why? The receiver should be alive for long.
                });
            }
        });
    });

    // the receiver reads all msgs from the queue
    drop(list_tx);
    let mut all_folder_count = 0;
    let mut all_file_count = 0;
    for msg in &list_rx {
        // the Result received from a thread will be propagated here
        let (folder_list, file_list) = msg?;
        all_folder_count += folder_list.len();
        all_file_count += file_list.len();
        folder_list_all.extend_from_slice(&folder_list);
        file_list_all.extend_from_slice(&file_list);
    }

    println_to_ui_thread_with_thread_name(&ui_tx, format!("remote list file sort {all_file_count}"), "R0");
    let string_file_list = crate::utils_mod::sort_list(file_list_all);
    file_list_source_files.write_append_str(&string_file_list)?;

    println_to_ui_thread_with_thread_name(&ui_tx, format!("remote list folder sort: {all_folder_count}"), "R0");
    let string_folder_list = crate::utils_mod::sort_list(folder_list_all);
    file_list_source_folders.write_append_str(&string_folder_list)?;

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

/// download one file
pub fn download_one_file(ui_tx: std::sync::mpsc::Sender<(String, ThreadName)>, ext_disk_base_path: &Path, path_to_download: &Path) -> Result<(), LibError> {
    let token = get_authorization_token()?;
    let client = dropbox_sdk::default_client::UserAuthDefaultClient::new(token);
    let thread_num = 0;
    download_internal(path_to_download, &client, ext_disk_base_path, thread_num, ui_tx)?;
    Ok(())
}

/// download one file with client object dropbox_sdk::default_client::UserAuthDefaultClient
fn download_internal(
    path_to_download: &Path,
    client: &dropbox_sdk::default_client::UserAuthDefaultClient,
    ext_disk_base_path: &Path,
    thread_num: i32,
    ui_tx: mpsc::Sender<(String, ThreadName)>,
) -> Result<(), LibError> {
    let mut bytes_out = 0u64;
    let thread_name = format!("R{thread_num}");
    let download_arg = dropbox_sdk::files::DownloadArg::new(path_to_download.to_string_lossy().to_string());

    let local_path = ext_disk_base_path.join(path_to_download.to_string_lossy().trim_start_matches("/"));
    // create folder if it does not exist
    let parent = local_path.parent().expect("Bug: Parent must exist.");
    if !parent.exists() {
        std::fs::create_dir_all(parent)?;
    }
    let base_temp_path_to_download = Path::new("temp_data/temp_download");
    if !base_temp_path_to_download.exists() {
        std::fs::create_dir_all(&base_temp_path_to_download)?;
    }
    let file_name = path_to_download.file_name().expect("Bug: Filename must exist.");
    let temp_local_path = base_temp_path_to_download.join(file_name);

    let mut file = std::fs::OpenOptions::new().create(true).write(true).open(&temp_local_path)?;

    let mut modified: Option<filetime::FileTime> = None;
    let mut s_modified;
    // I will download to a temp folder and then move the file to the right folder only when the download is complete.
    'download: loop {
        let result = dropbox_sdk::files::download(client, &download_arg, Some(bytes_out), None);
        match result {
            Ok(Ok(download_result)) => {
                let mut body = download_result.body.expect("Bug: body must exist");
                if modified.is_none() {
                    s_modified = download_result.result.client_modified.clone();
                    modified = Some(filetime::FileTime::from_system_time(humantime::parse_rfc3339(&s_modified)?));
                };
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
    let atime = modified.expect("Bug: modified date not exist");
    let mtime = modified.expect("Bug: modified date not exist");
    filetime::set_file_times(&temp_local_path, atime, mtime)?;

    // move the completed download file to his final folder
    // the classic std::fs::rename returns IoError: Invalid cross-device link (os error 18), because it cannot
    // move files from container to mounts in other operating systems.
    // I have to do copy with code and then delete.
    {
        let mut reader = std::fs::File::open(&temp_local_path)?;
        let mut writer = std::fs::File::create(local_path)?;
        std::io::copy(&mut reader, &mut writer)?;
    }
    std::fs::remove_file(&temp_local_path)?;
    Ok(())
}

/// download files from list
pub fn download_from_list(ui_tx: std::sync::mpsc::Sender<(String, ThreadName)>, ext_disk_base_path: &Path, file_list_for_download: &mut FileTxt) -> Result<(), LibError> {
    let list_for_download = file_list_for_download.read_to_string()?;
    let mut vec_list_for_download: Vec<&str> = list_for_download.lines().collect();

    if !list_for_download.is_empty() {
        match download_from_list_internal(ui_tx, ext_disk_base_path, &mut vec_list_for_download) {
            Ok(()) => {
                // in case all is ok, write actual situation to disk and continue
                file_list_for_download.empty()?;
                file_list_for_download.write_append_str(&vec_list_for_download.join("\n"))?;
            }
            Err(err) => {
                // also in case of error, write the actual situation to disk and return error
                file_list_for_download.empty()?;
                file_list_for_download.write_append_str(&vec_list_for_download.join("\n"))?;
                return Err(err);
            }
        }
    }
    Ok(())
}

fn download_from_list_internal(ui_tx: std::sync::mpsc::Sender<(String, ThreadName)>, ext_disk_base_path: &Path, vec_list_for_download: &mut Vec<&str>) -> Result<(), LibError> {
    let token = get_authorization_token()?;
    let client = dropbox_sdk::default_client::UserAuthDefaultClient::new(token);
    let client_ref = &client;
    // 3 threads to download in parallel
    let pool = rayon::ThreadPoolBuilder::new().num_threads(3).build().unwrap();
    pool.scope(|scoped| {
        let vec_list_for_download_clone = vec_list_for_download.clone();
        for line_path_to_download in vec_list_for_download_clone.iter() {
            let line: Vec<&str> = line_path_to_download.split("\t").collect();
            let path_to_download = line[0];
            let modified_for_download = line[1];
            let file_size: i32 = line[2].parse().expect("Bug: size must be in list_for_download");
            let thread_name = format!("R{}", rayon::current_thread_index().expect("Bug: thread num must exist."));
            if file_size == 0 {
                // create an empty file, because download empty file causes error 416
                let local_path = ext_disk_base_path.join(path_to_download.trim_start_matches("/"));
                let parent = local_path.parent().expect("Bug: parent must exist");
                if !parent.exists() {
                    std::fs::create_dir_all(parent).unwrap();
                }
                if local_path.exists() {
                    std::fs::remove_file(&local_path).expect("Bug: remove file must succeed");
                }
                let _file = FileTxt::open_for_read_and_write(&local_path).expect("Bug: open_for_read_and_write must succeed.");
                // change the file date
                let system_time = humantime::parse_rfc3339(modified_for_download).expect("Bug: parse_rfc3339 must succeed");
                let modified = filetime::FileTime::from_system_time(system_time);
                let atime = modified;
                let mtime = modified;
                filetime::set_file_times(&local_path, atime, mtime).expect("Bug: set_file_times must succeed.");
                println_to_ui_thread_with_thread_name(&ui_tx, local_path.to_string_lossy().to_string(), &thread_name);
            } else {
                let ui_tx_clone = ui_tx.clone();
                // execute in 3 separate threads, or waits for a free thread from the pool
                scoped.spawn(|_s| {
                    let thread_num = rayon::current_thread_index().expect("Bug: thread num must exist.");
                    download_internal(Path::new(path_to_download), client_ref, ext_disk_base_path, thread_num as i32, ui_tx_clone).expect("Bug: download_internal must succeed");
                });
            }
        }
    });

    Ok(())
}
