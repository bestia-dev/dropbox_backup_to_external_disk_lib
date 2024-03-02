[//]: # (auto_md_to_doc_comments segment start A)

# dropbox_backup_to_external_disk_lib

[//]: # (auto_cargo_toml_to_md start)

**One way sync from dropbox to external disc**  
***version: 2.1.70 date: 2024-02-18 author: [bestia.dev](https://bestia.dev) repository: [GitHub](https://github.com/bestia-dev/dropbox_backup_to_external_disk_lib/)***  

[//]: # (auto_cargo_toml_to_md end)

 ![maintained](https://img.shields.io/badge/maintained-green)
 ![work_in_progress](https://img.shields.io/badge/work_in_progress-yellow)

[//]: # (auto_lines_of_code start)
[![Lines in Rust code](https://img.shields.io/badge/Lines_in_Rust-1422-green.svg)](https://github.com/bestia-dev/dropbox_backup_to_external_disk_lib/)
[![Lines in Doc comments](https://img.shields.io/badge/Lines_in_Doc_comments-97-blue.svg)](https://github.com/bestia-dev/dropbox_backup_to_external_disk_lib/)
[![Lines in Comments](https://img.shields.io/badge/Lines_in_comments-185-purple.svg)](https://github.com/bestia-dev/dropbox_backup_to_external_disk_lib/)
[![Lines in examples](https://img.shields.io/badge/Lines_in_examples-0-yellow.svg)](https://github.com/bestia-dev/dropbox_backup_to_external_disk_lib/)
[![Lines in tests](https://img.shields.io/badge/Lines_in_tests-0-orange.svg)](https://github.com/bestia-dev/dropbox_backup_to_external_disk_lib/)

[//]: # (auto_lines_of_code end)

 [![License](https://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/bestia-dev/dropbox_backup_to_external_disk_lib/blob/main/LICENSE)
 [![Rust](https://github.com/bestia-dev/dropbox_backup_to_external_disk_lib/workflows/RustAction/badge.svg)](https://github.com/bestia-dev/dropbox_backup_to_external_disk_lib/)
 ![dropbox_backup_to_external_disk_lib](https://bestia.dev/webpage_hit_counter/get_svg_image/1784210611.svg)

Hashtags: #rustlang #tutorial #dropbox #cli #tui  
My projects on GitHub are more like a tutorial than a finished product: [bestia-dev tutorials](https://github.com/bestia-dev/tutorials_rust_wasm).

## Library project

This is the library project that contains all the programming logic, but none of the user interface. I will create different projects that contain only the user interface for this library. It will be different user interfaces to show how the same library can be used in different environments.  

## Motivation

On my Dropbox "remote drive" I have more than 1 Terabyte of data in 200 000 files.  
I own now 4 notebooks, 2 android phones and 1 tablet and not a single one has an internal drive with more than 1 Terabyte. I use Dropbox `Selective Sync`` to sync only the bare minimum I temporarily need on the local device. But I want to have a backup of all of my data. I must have a backup. Better, I want to have 2 backups of all the data on 2 external hard disks in different locations. So if Dropbox go bankrupt, I still have all my data.  
The original Dropbox Sync app works great for the internal HD, but is "not recommended" for external drives. I also need only one way sync: from remote to local. There exist apps for that:

- rclone
- dropbox_uploader

But I wanted to write something mine for fun, learning Rust and using my own apps.
I have a lot of files, so I want to list them first - remote and local. Then compare the lists and finally download the files.  
Obsolete files will "move to trash folder", so I can inspect what and how to delete manually.  
The dropbox remote storage will always be read_only, nothing will be modified there, never, no permission for that.  

## Development

I develop in my container image for Rust development inside WSL2 Debian Linux inside Win10. All this is described in my project [docker_rust_development](https://github.com/bestia-dev/docker_rust_development).  
I use [cargo-auto](https://crates.io/crates/cargo-auto) for automation tasks in Rust language.  
This is just a library project. There are projects for different user-interfaces that depend on this library. Compile and run them to test the application.  

Then I use WSL2 (Debian) on Win10 to execute the compiled program.  
The external disk path from WSL2 looks like this: `/mnt/d/DropBoxBackup1`.  
The CLI saves the list of the local files metadata in `temp_data/list_destination_files.csv`.  
And the list of the files metadata from the remote Dropbox to in `temp_data/list_source_files.csv`.
Tab delimited with metadata: path (with name), datetime modified, size.
The remote path is not really case-sensitive. They try to make it case-preserve, but this apply only to the last part of the path. Before that it is random-case.
For big dropbox remotes it can take a while to complete. After the first level folders are listed, I use 3 threads in a ThreadPool to get sub-folders recursively in parallel. It makes it much faster. Also the download of files is in parallel on multiple threads.  
TODO: If possible copy the local file that is synced with Dropbox instead of download.  
The sorting of lists is also done in parallel with the crate Rayon.  
Once the lists are complete the CLI will compare them and create files:  
`list_for_download.csv`  
`list_for_trash_files.csv`  
With this files the CLI will:  
`move_or_rename_local_files` if (name, size and file date) are equal, or (size, date and content_hash)
`trash_files` will move the obsolete files into a trash folder  
`download_from_list` - this can take a lot of time and it can be stopped with ctrl+c

## DropBox api2 - Stone sdk

Dropbox has made a `Stone` thingy that contains all the API definition. From there is possible to generate code boilerplate for different languages for the api-client.  
For Rust there is this quasi official project:  
<https://crates.io/crates/dropbox-sdk>  

## rename or move

Often a file is renamed or moved to another folder.  
I can try to recognize if there is the same file in list_for_trash_files and list_for_download.  
If the name, size and file date are equal then they are probably the same file.  
If the name is different, then try if content_hash is equal, but that is slow.  

## Learn something new every day

### REGEX adventure with non-breaking space and CRLF

We all know space. But there are other non-visible characters that are very similar and sometimes impossible to distinguish. Tab is one of them, but it is not so difficult to spot with a quick try.  
But nbsp non-breaking space, often used in HTML is a catastrophe. There is no way to tell it apart from the normal space. I used a regex to find a match with some spaces. It worked right for a years. Yesterday it didn't work. If I changed space to `\s` in the regex expression, it worked, but not with space. I tried everything and didn't find the cause. Finally I deleted and inserted the space. It works. But how? After a detailed analysis I discovered it was a non-breakable space. This is unicode 160 or \xa0, instead of normal space unicode 32 \x20. Now I will try to find them all and replace with normal space. What a crazy world.  
And another REGEX surprise. I try to have all text files delimited with the unix standard LF. But somehow the windows standard got mixed and I didn't recognize it. The regex for `end of line` $ didn't work for CRLF. When I changed it to LF, the good life is back and all works.

### Text files

Simple text files are a terrible way to store data that needs to be changed. It is ok for write once and then read. But there is not a good way to modify only one line inside a big text file. The recommended approach is read all, modify, save all. If the memory is not big enough then use a buffer to read a segment, modify, save a segment, repeat to end.  
There is another approach called memory map to file, but everybody is trying to avoid it because some other process could modify the file when in use and make it garbage.  
Sounds like a database is always a better choice for more agile development. In this project, I will create additional files that only append lines. Some kind of journal. And later use this to modify the big text files in one go. For example: list_just_downloaded.csv is added to list_destination_files.csv.  

### how to invert black-white in paint.net for dark theme

This is not same as `invert color`.  
Invert only black and white is for image transformation to `dark theme`.

1. Open image in PdN.
2. Duplicate layer.
3. Convert bottom layer to B/W.
4. Invert Colors bottom layer.
5. Adjust Contrast to 0 on top layer.
6. Change top layer blending mode to overlay.

## Windows PowerShell and UTF8

Incredible incredible incredible!  
From Debian, my program made a PowerShell script to change attributes of files: readonly and modified datetime.
The filenames contains unicode international characters. All should work fine,
But not !!!
When I copy filenames with "čšž" from Debian to the Powershell terminal it ignores the unicode characters.
What?  
We live in 2023 and PowerShell is like the modern spin of the Command Prompt and it does not work with unicode?
What a disappointment :-(

The version of PowerShell in Win10 in 5.1 and it is called internally "Windows PowerShell".

```powershell
$PSVersionTable
```

This version of PowerShell works in a specific code page that is NOT unicode, but CodePage: 20127, WindowsCodePage: 1252. Terrible choice. 

The new version of PowerShell is 7.1 and is called "PowerShell Core". The old PS cannot be upgraded to the new one. They must be installed side-by-side.

```powershell
winget search Microsoft.PowerShell
winget install --id Microsoft.Powershell --source winget
```

Now I have a separate terminal with PSVersion 7.4.0. It is not blue anymore, but black. Wow Microsoft!
Incredible! They make it work! Unicode works with PowerShell in 2023. Heureka!


## TODO

Can I recognize that a directory is moved or renamed? This is common and should be super fast.  
If most of the files in the directory are equal it means, that it is moved/renamed.  
Then a new `compare_files` will generate a new list if there are smaller differences.  
Is there a limit in the api for file size bigger than 2GB? Why the program crashes without an error?
Files with size 0 are not downloaded.
Solve empty folders in the program.
Make a command inside the program to save the oauth_token.  

## Dropbox basic account (free) for testing

I created a free basic account, so I can do testing and examples easily for this project.  

Generated access token for testing:
sl.BpJybWnGH0ZZ1974AAMnahDcyBcwt2_1RSUreTxYabTUNKuoQo4qszwca75_2M5vTzoc5_UbHtI1hux-51MDh4H2vfWwMuJJtf5LkhILJrESLnl7Wf0CORSZ-9snHTxPKUiLaEjU2t4c

## Open-source and free as a beer

My open-source projects are free as a beer (MIT license).  
I just love programming.  
But I need also to drink. If you find my projects and tutorials helpful, please buy me a beer by donating to my [PayPal](https://paypal.me/LucianoBestia).  
You know the price of a beer in your local bar ;-)  
So I can drink a free beer for your health :-)  
[Na zdravje!](https://translate.google.com/?hl=en&sl=sl&tl=en&text=Na%20zdravje&op=translate) [Alla salute!](https://dictionary.cambridge.org/dictionary/italian-english/alla-salute) [Prost!](https://dictionary.cambridge.org/dictionary/german-english/prost) [Nazdravlje!](https://matadornetwork.com/nights/how-to-say-cheers-in-50-languages/) 🍻

[//bestia.dev](https://bestia.dev)  
[//github.com/bestia-dev](https://github.com/bestia-dev)  
[//bestiadev.substack.com](https://bestiadev.substack.com)  
[//youtube.com/@bestia-dev-tutorials](https://youtube.com/@bestia-dev-tutorials)  

[//]: # (auto_md_to_doc_comments segment end A)
