// file_txt_mod.rs

use crossplatform_path::CrossPathBuf;

/// My object to work with text files.  
pub struct FileTxt {
    file_path: CrossPathBuf,
    file_txt: std::fs::File,
}

impl FileTxt {
    /// If file not exist, returns error.
    pub fn open_for_read(path: &CrossPathBuf) -> std::io::Result<Self> {
        let file = std::fs::File::options().read(true).open(path.to_path_buf_current_os())?;
        Ok(FileTxt {
            file_txt: file,
            file_path: path.to_owned(),
        })
    }

    /// If file not exist, it creates it.
    pub fn open_for_read_and_write(path: &CrossPathBuf) -> std::io::Result<Self> {
        if !path.exists() {
            std::fs::File::create(path.to_path_buf_current_os()).unwrap();
        }
        let file = std::fs::File::options()
            .read(true)
            .write(true)
            .open(path.to_path_buf_current_os())?;

        Ok(FileTxt {
            file_txt: file,
            file_path: path.to_owned(),
        })
    }

    /// Returns file path.
    pub fn file_path(&self) -> &CrossPathBuf {
        &self.file_path
    }

    /// Returns the final component of the Path, if there is one.
    pub fn file_name(&self) -> crossplatform_path::Result<String> {
        self.file_path.file_name()
    }

    /// This method is similar to fs::read_to_string, but instead of a path it expects a File parameter.  \
    ///
    /// So is possible to open a File in the bin part of the project and then pass it to the lib part of project.  \
    /// All input and output should be in the bin part of project and not in the lib.  
    pub fn read_to_string(&self) -> crossplatform_path::Result<String> {
        self.file_path.read_to_string()
    }

    /// Append str to file.
    pub fn write_append_str(&mut self, str: &str) -> std::io::Result<()> {
        std::io::Write::write_all(&mut self.file_txt, str.as_bytes())?;
        Ok(())
    }

    /// Empty the file.
    pub fn empty(&mut self) -> std::io::Result<()> {
        self.file_txt.set_len(0).unwrap();
        Ok(())
    }
}
