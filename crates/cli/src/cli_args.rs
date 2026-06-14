//! 极简命令行参数解析：仅处理数据源选择。
//! 支持：
//!   --sources env,file,sqlite     显式指定启用源与顺序
//!   --skip-source <name>          排除某源（可重复）
//! 二者都不传 → 默认 [Env, File, Sqlite]

use anyrouter_core::config_loader::SourceKind;

pub fn parse_sources<I: IntoIterator<Item = String>>(args: I) -> Vec<SourceKind> {
    let default = vec![SourceKind::Env, SourceKind::File, SourceKind::Sqlite];
    let mut explicit: Option<Vec<SourceKind>> = None;
    let mut skip: Vec<SourceKind> = Vec::new();

    let mut it = args.into_iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--sources" => {
                if let Some(val) = it.next() {
                    let list: Vec<SourceKind> =
                        val.split(',').filter_map(SourceKind::parse).collect();
                    if !list.is_empty() {
                        explicit = Some(list);
                    }
                }
            }
            "--skip-source" => {
                if let Some(val) = it.next() {
                    if let Some(k) = SourceKind::parse(&val) {
                        skip.push(k);
                    }
                }
            }
            _ => {}
        }
    }

    let base = explicit.unwrap_or(default);
    base.into_iter().filter(|k| !skip.contains(k)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(s: &[&str]) -> Vec<String> {
        s.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn default_is_all_three() {
        assert_eq!(
            parse_sources(v(&[])),
            vec![SourceKind::Env, SourceKind::File, SourceKind::Sqlite]
        );
    }

    #[test]
    fn explicit_sources_respect_order() {
        assert_eq!(
            parse_sources(v(&["--sources", "sqlite,env"])),
            vec![SourceKind::Sqlite, SourceKind::Env]
        );
    }

    #[test]
    fn skip_source_removes() {
        assert_eq!(
            parse_sources(v(&["--skip-source", "file"])),
            vec![SourceKind::Env, SourceKind::Sqlite]
        );
    }
}
