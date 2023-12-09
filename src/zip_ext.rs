use std::path::PathBuf;

use chrono::{DateTime, Local, LocalResult, TimeZone};
use zip::read::ZipFile;

use crate::encoding::ZipEncoding;

pub trait ZipFileExt<'a> {
    fn decoded_name_lossy(&self, encoding: ZipEncoding) -> PathBuf;
    fn is_utf8(&self) -> bool;

    fn last_modified_chrono(&self) -> LocalResult<DateTime<Local>>;
}

impl<'a> ZipFileExt<'a> for ZipFile<'a> {
    fn decoded_name_lossy(&self, encoding: ZipEncoding) -> PathBuf {
        if self.is_utf8() {
            return PathBuf::from(self.name());
        }
        match encoding {
            ZipEncoding::Cp437 => PathBuf::from(self.name()),
            ZipEncoding::EncodingRs(encoding) => {
                let (decoded_name_cow, _, _malformed) = encoding.decode(self.name_raw());
                let decoded_name = decoded_name_cow.as_ref();
                PathBuf::from(decoded_name)
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
        // https://github.com/zip-rs/zip/blob/3e88fe66c941d411cff5cf49778ba08c2ed93801/src/read.rs#L671
        let (utf8_cow, _encoding, malformed) = encoding_rs::UTF_8.decode(self.name_raw());
        return !malformed && self.name() == utf8_cow;
    }

    fn last_modified_chrono(&self) -> LocalResult<DateTime<Local>> {
        let zip_dt = self.last_modified();
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
