// file_txt_mod.rs

/// object to work with text files
pub struct FileTxt {
    file_txt: std::fs::File,
}
impl FileTxt {
    pub fn from_file(file: std::fs::File) -> Self {
        FileTxt { file_txt: file }
    }
    /// if file not exist, it creates it
    pub fn open_for_read_and_write(path: &str) -> std::io::Result<Self> {
        let path = std::path::Path::new(path);
        if !path.exists() {
            std::fs::File::create(path).unwrap();
        }
        let file = std::fs::File::options().read(true).write(true).open(path)?;

        Ok(Self::from_file(file))
    }

    /// This method is similar to fs::read_to_string, but instead of a path it expects a File parameter
    /// So is possible to open a File in the bin part of the project and then pass it to the lib part of project.
    /// All input and output should be in the bin part of project and not in the lib.
    pub fn read_to_string(&mut self) -> std::io::Result<String> {
        let size = self.file_txt.metadata().map(|m| m.len()).unwrap_or(0);
        let mut string = String::with_capacity(size as usize);
        std::io::Read::read_to_string(&mut self.file_txt, &mut string)?;
        Ok(string)
    }

    /// write str to file (append)
    pub fn write_str(&mut self, str: &str) -> std::io::Result<()> {
        std::io::Write::write_all(&mut self.file_txt, str.as_bytes())?;
        Ok(())
    }

    /// empty the file
    pub fn empty(&mut self) -> std::io::Result<()> {
        self.file_txt.set_len(0).unwrap();
        Ok(())
    }
}
