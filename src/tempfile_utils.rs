use std::io;
use std::path::{Path, PathBuf};

use tempfile::{Builder, TempDir};

pub trait TempDirExt {
    fn relative_path_from<P: AsRef<Path>>(&self, from: P) -> PathBuf;
}

impl TempDirExt for TempDir {
    fn relative_path_from<P: AsRef<Path>>(&self, from: P) -> PathBuf {
        let cwd = from.as_ref().canonicalize().unwrap();
        pathdiff::diff_paths(self.path(), cwd).unwrap()
    }
}

pub fn tempdir_with_prefix_in(path: &Path, prefix: &str) -> io::Result<TempDir> {
    Builder::new().prefix(prefix).tempdir_in(path)
}
