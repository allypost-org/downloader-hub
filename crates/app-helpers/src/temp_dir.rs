use std::{
    env,
    ffi::OsString,
    marker::Send,
    path::{Path, PathBuf},
};

use super::id::time_thread_id;

#[derive(Debug)]
pub struct TempDir {
    path: PathBuf,
    delete_on_drop: bool,
}
impl TempDir {
    pub fn absolute<T>(absolute_dir_path: T) -> Result<Self, std::io::Error>
    where
        T: Into<PathBuf>,
    {
        let tmp_dir = absolute_dir_path.into();

        if !tmp_dir.exists() {
            std::fs::create_dir_all(&tmp_dir)?;
        }

        if !tmp_dir.is_dir() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                "Path exists and is not a directory",
            ));
        }

        Ok(Self {
            path: tmp_dir,
            delete_on_drop: true,
        })
    }

    pub fn in_tmp<T>(dir_name: T) -> Result<Self, std::io::Error>
    where
        T: Into<OsString>,
    {
        let tmp_dir = env::temp_dir();
        let tmp_dir = tmp_dir.join(dir_name.into());

        Self::absolute(tmp_dir)
    }

    pub fn in_tmp_with_prefix<T>(dir_name_prefix: T) -> Result<Self, std::io::Error>
    where
        T: Into<OsString> + Send,
    {
        let mut f: OsString = dir_name_prefix.into();
        f.push(time_thread_id());
        Self::in_tmp(f)
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    #[allow(dead_code)]
    pub const fn no_delete_on_drop(&mut self) -> &mut Self {
        self.delete_on_drop = false;
        self
    }

    #[must_use]
    pub const fn with_no_delete_on_drop(mut self) -> Self {
        self.no_delete_on_drop();
        self
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        if self.delete_on_drop {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }
}
