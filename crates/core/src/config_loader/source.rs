use anyhow::Result;

use super::types::RawConfig;

/// 数据源种类
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceKind {
    Env,
    File,
    Sqlite,
}

impl SourceKind {
    /// 解析源名（大小写不敏感）；db 是 sqlite 的别名。
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "env" => Some(Self::Env),
            "file" => Some(Self::File),
            "sqlite" | "db" => Some(Self::Sqlite),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Env => "env",
            Self::File => "file",
            Self::Sqlite => "sqlite",
        }
    }
}

/// 单个数据源；load 失败由调用方记日志后跳过，不中断整体加载。
pub trait ConfigSource {
    fn kind(&self) -> SourceKind;
    fn load(&self) -> Result<RawConfig>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_source_names() {
        assert_eq!(SourceKind::parse("ENV"), Some(SourceKind::Env));
        assert_eq!(SourceKind::parse(" file "), Some(SourceKind::File));
        assert_eq!(SourceKind::parse("db"), Some(SourceKind::Sqlite));
        assert_eq!(SourceKind::parse("sqlite"), Some(SourceKind::Sqlite));
        assert_eq!(SourceKind::parse("xxx"), None);
    }
}
