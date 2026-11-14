use std::io::Read;
use std::path::PathBuf;

use chrono::{DateTime, Local, LocalResult, TimeZone};
use zip::read::ZipFile;

use crate::encoding::ZipEncoding;

pub trait ZipFileExt<'a, R: Read> {
    fn decoded_name_lossy(&self, encoding: ZipEncoding) -> Option<PathBuf>;
    fn is_utf8(&self) -> bool;

    fn last_modified_chrono(&self) -> LocalResult<DateTime<Local>>;
}

impl<'a, R: Read> ZipFileExt<'a, R> for ZipFile<'a, R> {
    fn decoded_name_lossy(&self, encoding: ZipEncoding) -> Option<PathBuf> {
        if self.is_utf8() {
            return self.enclosed_name();
        }
        match encoding {
            ZipEncoding::Cp437 => self.enclosed_name(),
            ZipEncoding::EncodingRs(encoding) => {
                // The enclosed name is unnecessary here because
                // callers are responsible for sanitizing the path.
                let (decoded_name_cow, _, _malformed) = encoding.decode(self.name_raw());
                let decoded_name = decoded_name_cow.as_ref();
                Some(PathBuf::from(decoded_name))
            }
        }
    }

    fn is_utf8(&self) -> bool {
        // The current implementation doesn't use Language encoding
        // flag (Bit 11 of general purpose bit flag) which means the
        // filename is encoded by utf-8.  zip crate does not reveal
        // the flag but we can get the offset of the central header by
        // ZipFile::central_header_start.  However ZipFile cannot
        // access to the zip file reader.
        // https://github.com/zip-rs/zip2/blob/v7.0.0/src/read.rs#L1375
        let (utf8_cow, _encoding, malformed) = encoding_rs::UTF_8.decode(self.name_raw());
        let Ok(name) = self.name() else { return false };
        !malformed && name == utf8_cow
    }

    fn last_modified_chrono(&self) -> LocalResult<DateTime<Local>> {
        let Some(zip_dt) = self.last_modified() else {
            return LocalResult::None;
        };
        Local.with_ymd_and_hms(
            zip_dt.year().into(),
            zip_dt.month().into(),
            zip_dt.day().into(),
            zip_dt.hour().into(),
            zip_dt.minute().into(),
            zip_dt.second().into(),
        )
    }
}
