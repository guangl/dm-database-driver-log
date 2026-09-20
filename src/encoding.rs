//! 日志文件编码配置与解码。

use encoding::all::GB18030;
use encoding::{DecoderTrap, Encoding};

/// 文件编码提示，用于指示日志文件的字符编码。
#[derive(Copy, Clone, Debug, PartialEq, Eq, Default)]
pub enum FileEncodingHint {
    /// 自动探测：每行优先按 UTF-8，失败后按 GB18030 解码。
    #[default]
    Auto,
    /// 文件使用 UTF-8 编码。
    Utf8,
    /// 文件使用 GB18030 编码。
    Gb18030,
}

/// 将一行日志字节解码成可供解析器借用的 UTF-8 字符串。
///
/// 驱动日志通常是 UTF-8，但 DM 部署中也可能出现 GB18030 文件。严格解码
/// 失败时退回 lossy UTF-8，确保一行坏字符不会让整个流式迭代器失去同步。
pub(crate) fn decode(bytes: &[u8], hint: FileEncodingHint) -> String {
    match hint {
        FileEncodingHint::Utf8 => String::from_utf8_lossy(bytes).into_owned(),
        FileEncodingHint::Gb18030 => decode_gb18030(bytes),
        FileEncodingHint::Auto => match std::str::from_utf8(bytes) {
            Ok(text) => text.to_owned(),
            Err(_) => decode_gb18030(bytes),
        },
    }
}

fn decode_gb18030(bytes: &[u8]) -> String {
    GB18030
        .decode(bytes, DecoderTrap::Strict)
        .unwrap_or_else(|_| String::from_utf8_lossy(bytes).into_owned())
}
