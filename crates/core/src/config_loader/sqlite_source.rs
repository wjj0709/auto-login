use std::path::PathBuf;

use anyhow::Result;

use super::source::{ConfigSource, SourceKind};
use super::types::{RawAccount, RawConfig, RawSite};
use crate::storage::Storage;

pub struct SqliteSource {
    db_path: PathBuf,
}

impl SqliteSource {
    pub fn new(db_path: PathBuf) -> Self {
        Self { db_path }
    }
}

impl ConfigSource for SqliteSource {
    fn kind(&self) -> SourceKind {
        SourceKind::Sqlite
    }

    fn load(&self) -> Result<RawConfig> {
        let storage = Storage::open(&self.db_path)?;
        let sites_db = storage.list_sites()?;

        let mut sites = Vec::new();
        let mut accounts = Vec::new();
        for s in &sites_db {
            sites.push(RawSite {
                name: s.name.clone(),
                domain: s.domain.clone(),
                login_path: s.login_path.clone(),
                sign_in_path: s.sign_in_path.clone(),
                user_info_path: s.user_info_path.clone(),
                tokens_path: s.tokens_path.clone(),
                logs_path: s.logs_path.clone(),
                chart_path: s.chart_path.clone(),
                api_user_key: s.api_user_key.clone(),
            });
            for a in storage.list_accounts_by_site(s.id)? {
                accounts.push(RawAccount {
                    site_name: s.name.clone(),
                    api_user: a.api_user,
                    display_name: Some(a.name),
                    cookies: a.cookies,
                    username: a.username,
                    password: a.password,
                });
            }
        }
        // SQLite 当前不存邮件配置
        Ok(RawConfig {
            sites,
            accounts,
            email: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{AccountInput, SiteInput};

    #[test]
    fn reads_sites_and_accounts_from_db() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("t.db");
        {
            let storage = Storage::open(&db).unwrap();
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
                    name: "acc".into(),
                    api_user: "99".into(),
                    username: None,
                    password: None,
                    cookies: None,
                })
                .unwrap();
        }
        let cfg = SqliteSource::new(db).load().unwrap();
        assert_eq!(cfg.sites.len(), 1);
        assert_eq!(cfg.accounts.len(), 1);
        assert_eq!(cfg.accounts[0].site_name, "S");
        assert_eq!(cfg.accounts[0].api_user, "99");
    }
}
