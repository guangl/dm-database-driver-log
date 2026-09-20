#![cfg(feature = "jdbc")]

use dm_database_driver_log::{FileEncodingHint, parse_bytes_with_encoding};

const UTF8_LINE: &str = "[INFO - 2026-09-16 17:45:19.763] tid:119 - [线程] { conn-3 } close();";

#[test]
fn public_byte_api_decodes_utf8_gb18030_and_invalid_bytes() {
    use ::encoding::all::GB18030;
    use ::encoding::{EncoderTrap, Encoding};

    let utf8 = parse_bytes_with_encoding(UTF8_LINE.as_bytes(), FileEncodingHint::Utf8).unwrap();
    assert_eq!(utf8.as_jdbc().unwrap().thread, "线程");

    let gb18030 = GB18030.encode(UTF8_LINE, EncoderTrap::Strict).unwrap();
    let decoded = parse_bytes_with_encoding(&gb18030, FileEncodingHint::Gb18030).unwrap();
    assert_eq!(decoded.as_jdbc().unwrap().thread, "线程");

    let auto = parse_bytes_with_encoding(&gb18030, FileEncodingHint::Auto).unwrap();
    assert_eq!(auto.as_jdbc().unwrap().thread, "线程");

    let mut invalid_utf8 = b"[INFO - 2026-09-16 17:45:19.763] tid:119 - [".to_vec();
    invalid_utf8.push(0xff);
    invalid_utf8.extend_from_slice(b"] { conn-3 } close();");
    let lossy = parse_bytes_with_encoding(&invalid_utf8, FileEncodingHint::Utf8).unwrap();
    assert_eq!(lossy.as_jdbc().unwrap().thread, "�");

    let lossy_auto = parse_bytes_with_encoding(&[0xff], FileEncodingHint::Auto);
    assert!(lossy_auto.is_err());
}
