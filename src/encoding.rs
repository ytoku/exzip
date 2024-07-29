use std::collections::HashMap;
use std::sync::LazyLock;

use encoding_rs::Encoding;

#[derive(Clone, Copy)]
pub enum ZipEncoding {
    Cp437,
    EncodingRs(&'static Encoding),
}

static NAME_TABLE: LazyLock<HashMap<&'static str, ZipEncoding>> = LazyLock::new(|| {
    let mut m = HashMap::new();
    m.insert("cp437", ZipEncoding::Cp437);
    m.insert("cp932", ZipEncoding::EncodingRs(encoding_rs::SHIFT_JIS));
    m
});

pub fn get_encoding(name: &str) -> Option<ZipEncoding> {
    let name_label = name.as_bytes();
    let from_name_table = NAME_TABLE.get(&name.to_lowercase() as &str);
    if let Some(&encoding) = from_name_table {
        Some(encoding)
    } else {
        Encoding::for_label(name_label).map(ZipEncoding::EncodingRs)
    }
}
