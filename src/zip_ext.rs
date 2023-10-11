use std::path::PathBuf;

use zip::read::ZipFile;

use crate::encoding::ZipEncoding;

pub trait ZipFileExt<'a> {
    fn decoded_name_lossy(&self, encoding: ZipEncoding) -> PathBuf;
}

impl<'a> ZipFileExt<'a> for ZipFile<'a> {
    fn decoded_name_lossy(&self, encoding: ZipEncoding) -> PathBuf {
        match encoding {
            ZipEncoding::Cp437 => PathBuf::from(self.name()),
            ZipEncoding::EncodingRs(encoding) => {
                let (decoded_name_cow, _, _malformed) = encoding.decode(self.name_raw());
                let decoded_name = decoded_name_cow.as_ref();
                PathBuf::from(decoded_name)
            }
        }
    }
}
