use std::collections::HashMap;
use std::sync::OnceLock;

use encoding_rs::Encoding;

static NAME_TABLE: OnceLock<HashMap<&'static str, &'static [u8]>> = OnceLock::new();

fn init_name_table() -> HashMap<&'static str, &'static [u8]> {
    let mut m = HashMap::new();
    m.insert("cp932", b"shift_jis" as &'static [u8]);
    m
}

pub fn get_encoding(name: &str) -> Option<&'static Encoding> {
    let name_label = name.as_bytes();
    let label: &[u8] = NAME_TABLE
        .get_or_init(init_name_table)
        .get(name)
        .unwrap_or(&name_label);
    Encoding::for_label(label)
}
