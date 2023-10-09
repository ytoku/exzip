use std::path::PathBuf;

use encoding_rs::Encoding;
use zip::read::ZipFile;

pub trait ZipFileExt<'a> {
    fn decoded_name_lossy(&self, encoding: &'static Encoding) -> PathBuf;
}

impl<'a> ZipFileExt<'a> for ZipFile<'a> {
    fn decoded_name_lossy(&self, encoding: &'static Encoding) -> PathBuf {
        let (decoded_name_cow, _, _malformed) = encoding.decode(self.name_raw());
        let decoded_name = decoded_name_cow.as_ref();
        PathBuf::from(decoded_name)
    }
}
