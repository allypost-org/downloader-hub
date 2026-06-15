use std::{
    ffi::OsString,
    fs::File,
    path::{Path, PathBuf},
};

use super::id::time_thread_id;
use crate::config::HelpersConfig;

#[derive(Debug)]
pub struct TempFile {
    path: PathBuf,
    file: File,
    delete_on_drop: bool,
}
impl TempFile {
    pub fn new<T>(file_name: T) -> Result<Self, std::io::Error>
    where
        T: Into<OsString> + std::marker::Send,
    {
        let tmp_dir = HelpersConfig::cache_dir();

        if !tmp_dir.exists() {
            std::fs::create_dir_all(&tmp_dir)?;
        }

        if !tmp_dir.is_dir() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Cache directory is not a directory",
            ));
        }

        let tmp_file = tmp_dir.join(file_name.into());
        let file = File::create(&tmp_file)?;

        Ok(Self {
            path: tmp_file,
            file,
            delete_on_drop: true,
        })
    }

    pub fn new_with_prefix<T>(file_name_prefix: T) -> Result<Self, std::io::Error>
    where
        T: Into<OsString> + std::marker::Send,
    {
        let mut f: OsString = file_name_prefix.into();
        f.push(time_thread_id());
        Self::new(f)
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub const fn file_mut(&mut self) -> &mut File {
        &mut self.file
    }

    pub fn try_clone_file(&self) -> Result<File, std::io::Error> {
        self.file.try_clone()
    }

    #[allow(dead_code)]
    pub const fn no_delete_on_drop(&mut self) -> &mut Self {
        self.delete_on_drop = false;
        self
    }
}

impl AsRef<Path> for TempFile {
    fn as_ref(&self) -> &Path {
        self.path.as_ref()
    }
}

impl AsRef<PathBuf> for TempFile {
    fn as_ref(&self) -> &PathBuf {
        &self.path
    }
}

impl AsRef<File> for TempFile {
    fn as_ref(&self) -> &File {
        &self.file
    }
}

impl Drop for TempFile {
    fn drop(&mut self) {
        if self.delete_on_drop {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}
