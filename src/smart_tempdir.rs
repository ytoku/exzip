use std::fs;
use std::io;
use std::mem;
use std::path::Path;
use tempfile::{Builder, TempDir};

pub struct SmartTempDir {
    temp_dir: Option<TempDir>,
}

impl SmartTempDir {
    pub fn path(&self) -> &Path {
        self.temp_dir.as_ref().unwrap().path()
    }

    fn remove_all(&mut self) -> io::Result<()> {
        if let Some(temp_dir) = self.temp_dir.take() {
            let path = temp_dir.path();
            if path.exists() {
                fs::remove_dir_all(path).expect("Failed to clean the temporary directory");
            }
        }
        Ok(())
    }

    pub fn close(mut self) -> io::Result<()> {
        self.remove_all()?;
        mem::forget(self);

        Ok(())
    }
}

impl Drop for SmartTempDir {
    fn drop(&mut self) {
        let _ = self.remove_all();
    }
}

pub fn smart_tempdir_in(path: &Path, prefix: &str) -> io::Result<SmartTempDir> {
    let temp_dir = Builder::new().prefix(prefix).tempdir_in(path)?;
    Ok(SmartTempDir {
        temp_dir: Some(temp_dir),
    })
}
