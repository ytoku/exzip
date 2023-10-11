use std::path::PathBuf;

use chrono::{DateTime, Local, LocalResult, TimeZone};
use zip::read::ZipFile;

use crate::encoding::ZipEncoding;

pub trait ZipFileExt<'a> {
    fn decoded_name_lossy(&self, encoding: ZipEncoding) -> PathBuf;

    fn last_modified_chrono(&self) -> LocalResult<DateTime<Local>>;
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
