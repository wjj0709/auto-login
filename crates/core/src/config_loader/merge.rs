use std::collections::BTreeMap;

use anyhow::Result;

use super::types::{RawAccount, RawConfig, RawEmail, RawSite};
use crate::models::{AccountInput, SiteInput};
use crate::storage::Storage;

/// 按读取顺序合并多个源（靠后的覆盖同键），返回去重后的配置。
/// 调用方需保证 configs 的顺序即优先级从低到高。
pub fn merge_configs(configs: &[RawConfig]) -> RawConfig {
    // 站点按 name 去重，后源覆盖
    let mut sites: BTreeMap<String, RawSite> = BTreeMap::new();
    // 账户按 (site_name, api_user) 去重，后源覆盖
    let mut accounts: BTreeMap<(String, String), RawAccount> = BTreeMap::new();
    let mut email: Option<RawEmail> = None;

    for cfg in configs {
        for s in &cfg.sites {
            sites.insert(s.name.clone(), s.clone());
        }
        for a in &cfg.accounts {
            accounts.insert(a.key(), a.clone());
        }
        if let Some(e) = &cfg.email {
            match email.as_mut() {
                Some(existing) => existing.overlay(e),
                None => email = Some(e.clone()),
            }
        }
    }

    RawConfig {
        sites: sites.into_values().collect(),
        accounts: accounts.into_values().collect(),
        email,
    }
}

/// 把合并结果中「SQLite 没有」的站点/账户增量插入库；已有的不动。
/// 返回插入的 (站点数, 账户数)。
pub fn writeback_to_sqlite(storage: &Storage, merged: &RawConfig) -> Result<(usize, usize)> {
    let mut inserted_sites = 0;
    let mut inserted_accounts = 0;

    // 1. 站点：库中无同名则插入
    let existing_sites = storage.list_sites()?;
    for s in &merged.sites {
        if existing_sites.iter().any(|x| x.name == s.name) {
            continue;
        }
        storage.insert_site(&SiteInput {
            name: s.name.clone(),
            domain: s.domain.clone(),
            login_path: s.login_path.clone(),
            sign_in_path: s.sign_in_path.clone(),
            user_info_path: s.user_info_path.clone(),
            tokens_path: s.tokens_path.clone(),
            logs_path: s.logs_path.clone(),
            chart_path: s.chart_path.clone(),
            api_user_key: s.api_user_key.clone(),
        })?;
        inserted_sites += 1;
    }

    // 2. 账户：按 site_name 找 site_id；库中 (site_id, api_user) 无对应则插入
    let sites_now = storage.list_sites()?;
    for a in &merged.accounts {
        let Some(site) = sites_now.iter().find(|x| x.name == a.site_name) else {
            // 账户引用了不存在的站点，跳过
            eprintln!("[config_loader] 跳过账户：站点 '{}' 不存在", a.site_name);
            continue;
        };
        let existing = storage.list_accounts_by_site(site.id)?;
        if existing.iter().any(|x| x.api_user == a.api_user) {
            continue;
        }
        storage.insert_account(&AccountInput {
            site_id: site.id,
            name: a.display_name.clone().unwrap_or_else(|| a.api_user.clone()),
            api_user: a.api_user.clone(),
            username: a.username.clone(),
            password: a.password.clone(),
            cookies: a.cookies.clone(),
        })?;
        inserted_accounts += 1;
    }

    Ok((inserted_sites, inserted_accounts))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn site(name: &str, domain: &str) -> RawSite {
        RawSite::with_defaults(name, domain, None)
    }
    fn acct(site: &str, api: &str, name: &str) -> RawAccount {
        RawAccount {
            site_name: site.into(),
            api_user: api.into(),
            display_name: Some(name.into()),
            cookies: None,
            username: None,
            password: None,
        }
    }

    #[test]
    fn later_source_overrides_same_key() {
        let env = RawConfig {
            sites: vec![site("S", "https://old.com")],
            accounts: vec![acct("S", "1", "envname")],
            email: None,
        };
        let sqlite = RawConfig {
            sites: vec![site("S", "https://new.com")],
            accounts: vec![acct("S", "1", "dbname")],
            email: None,
        };
        // 顺序：env(低) → sqlite(高)
        let merged = merge_configs(&[env, sqlite]);
        assert_eq!(merged.sites.len(), 1);
        assert_eq!(merged.sites[0].domain, "https://new.com"); // 后源优先
        assert_eq!(merged.accounts.len(), 1);
        assert_eq!(merged.accounts[0].display_name.as_deref(), Some("dbname"));
    }

    #[test]
    fn different_keys_accumulate() {
        let a = RawConfig {
            accounts: vec![acct("S", "1", "a")],
            ..Default::default()
        };
        let b = RawConfig {
            accounts: vec![acct("S", "2", "b")],
            ..Default::default()
        };
        let merged = merge_configs(&[a, b]);
        assert_eq!(merged.accounts.len(), 2);
    }

    #[test]
    fn email_field_level_overlay() {
        let env = RawConfig {
            email: Some(RawEmail {
                user: "u".into(),
                ..Default::default()
            }),
            ..Default::default()
        };
        let file = RawConfig {
            email: Some(RawEmail {
                pass: "p".into(),
                ..Default::default()
            }),
            ..Default::default()
        };
        let merged = merge_configs(&[env, file]);
        let e = merged.email.unwrap();
        assert_eq!(e.user, "u");
        assert_eq!(e.pass, "p");
    }

    #[test]
    fn writeback_inserts_new_and_keeps_existing() {
        use crate::models::{AccountInput, SiteInput};
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("w.db");
        let storage = Storage::open(&db).unwrap();
        // 库中已有 S + 账户 (S,1)，display name = "old"
        let sid = storage
            .insert_site(&SiteInput {
                name: "S".into(),
                domain: "https://s.com".into(),
                login_path: "/login".into(),
                sign_in_path: None,
                user_info_path: "/api/user/self".into(),
                tokens_path: "/api/token/".into(),
                logs_path: "/api/log/self".into(),
                chart_path: "/api/data/self".into(),
                api_user_key: "new-api-user".into(),
            })
            .unwrap();
        storage
            .insert_account(&AccountInput {
                site_id: sid,
                name: "old".into(),
                api_user: "1".into(),
                username: None,
                password: None,
                cookies: None,
            })
            .unwrap();

        // 合并结果含已有 (S,1) 改名 + 新账户 (S,2)
        let merged = RawConfig {
            sites: vec![site("S", "https://s.com")],
            accounts: vec![acct("S", "1", "newname"), acct("S", "2", "b")],
            email: None,
        };
        let (si, ai) = writeback_to_sqlite(&storage, &merged).unwrap();
        assert_eq!(si, 0); // 站点已存在
        assert_eq!(ai, 1); // 仅新增 (S,2)

        let accts = storage.list_accounts_by_site(sid).unwrap();
        assert_eq!(accts.len(), 2);
        // 已有 (S,1) 未被覆盖，仍是 "old"
        let a1 = accts.iter().find(|a| a.api_user == "1").unwrap();
        assert_eq!(a1.name, "old");
    }
}
