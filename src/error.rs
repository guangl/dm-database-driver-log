use thiserror::Error;

/// 解析失败的位置与原因。
///
/// `line_number == 0` 表示错误来自独立的解析调用，而不是文件迭代器。
/// 错误只在失败路径保存原文，不影响正常解析路径。
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ParseError {
    #[error("invalid format at line {line_number}, offset {offset}: {what} | raw: {raw}")]
    InvalidFormat {
        raw: String,
        line_number: u64,
        offset: usize,
        what: &'static str,
    },
    #[error("IO error: {0}")]
    IoError(String),
}

impl ParseError {
    #[cfg(any(feature = "jdbc", feature = "dm-provider"))]
    pub(crate) fn invalid(offset: usize, what: &'static str) -> Self {
        Self::InvalidFormat {
            raw: String::new(),
            line_number: 0,
            offset,
            what,
        }
    }

    pub(crate) fn with_context(mut self, raw: &str, line_number: u64) -> Self {
        if let Self::InvalidFormat {
            raw: saved,
            line_number: saved_line,
            ..
        } = &mut self
        {
            if saved.is_empty() {
                *saved = raw.to_owned();
            }
            *saved_line = line_number;
        }
        self
    }

    pub fn line_number(&self) -> Option<u64> {
        match self {
            Self::InvalidFormat { line_number: 0, .. } => None,
            Self::InvalidFormat { line_number, .. } => Some(*line_number),
            Self::IoError(_) => None,
        }
    }

    pub fn offset(&self) -> Option<usize> {
        match self {
            Self::InvalidFormat { offset, .. } => Some(*offset),
            Self::IoError(_) => None,
        }
    }
}
