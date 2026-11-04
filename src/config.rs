//! ~/.config/lg/config.toml: hosts, accounts, capabilities, aliases and settings.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Debug, Default, Serialize, Deserialize, Clone, PartialEq)]
pub struct Config {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_host: Option<String>,
    #[serde(default)]
    pub settings: BTreeMap<String, String>,
    #[serde(default)]
    pub aliases: BTreeMap<String, String>,
    #[serde(default)]
    pub hosts: BTreeMap<String, Host>,
}

#[derive(Debug, Default, Serialize, Deserialize, Clone, PartialEq)]
pub struct Host {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_user: Option<String>,
    #[serde(default)]
    pub users: BTreeMap<String, User>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<Caps>,
}

#[derive(Debug, Default, Serialize, Deserialize, Clone, PartialEq)]
pub struct User {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_id: Option<i64>,
    /// Only set when the system keyring is unavailable (file is then mode 600).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
}

/// What the instance's API documentation (/api-docs/v1|v2/swagger.yaml) lists.
#[derive(Debug, Default, Serialize, Deserialize, Clone, PartialEq)]
pub struct Caps {
    pub checked: String,
    pub v1: bool,
    pub v2: bool,
    /// "METHOD /api/vN/path/{id}"; empty when the documentation could not be read.
    #[serde(default)]
    pub endpoints: Vec<String>,
}

pub const SETTINGS: &[(&str, &str, &str)] = &[
    ("browser", "", "Command used to open web pages (default: system browser)"),
    ("prompt", "enabled", "Interactive prompts: enabled or disabled"),
];

pub fn dir() -> PathBuf {
    let env = |k| std::env::var_os(k).filter(|v| !v.is_empty()).map(PathBuf::from);
    if let Some(d) = env("LG_CONFIG_DIR") {
        return d;
    }
    if let Some(d) = env("XDG_CONFIG_HOME") {
        return d.join("lg");
    }
    if cfg!(windows)
        && let Some(d) = env("APPDATA")
    {
        return d.join("lg");
    }
    env("HOME")
        .or_else(|| env("USERPROFILE"))
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".config")
        .join("lg")
}

pub fn path() -> PathBuf {
    dir().join("config.toml")
}

impl Config {
    pub fn load() -> Result<Config> {
        let p = path();
        match std::fs::read_to_string(&p) {
            Ok(s) => toml::from_str(&s).with_context(|| {
                tr!(
                    format!("{} is not valid TOML; fix or delete it", p.display()),
                    format!("{} n'est pas un TOML valide : corrigez-le ou supprimez-le", p.display())
                )
            }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Config::default()),
            Err(e) => Err(e).with_context(|| p.display().to_string()),
        }
    }

    pub fn save(&self) -> Result<()> {
        let d = dir();
        std::fs::create_dir_all(&d).with_context(|| d.display().to_string())?;
        let p = path();
        let tmp = p.with_extension("toml.tmp");
        std::fs::write(&tmp, toml::to_string_pretty(self)?)?;
        restrict(&tmp)?;
        std::fs::rename(&tmp, &p).with_context(|| p.display().to_string())?;
        Ok(())
    }

    pub fn setting(&self, key: &str) -> String {
        self.settings
            .get(key)
            .cloned()
            .unwrap_or_else(|| SETTINGS.iter().find(|s| s.0 == key).map(|s| s.1.to_string()).unwrap_or_default())
    }
}

#[cfg(unix)]
fn restrict(p: &std::path::Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o600))?;
    Ok(())
}

// shortcut: on Windows the file keeps the default ACL of the user profile, which is per-user already
#[cfg(not(unix))]
fn restrict(_: &std::path::Path) -> Result<()> {
    Ok(())
}

/// "https://cle.inserm.fr/" → "cle.inserm.fr"; http:// URLs (local tests) keep their scheme.
pub fn normalize_host(h: &str) -> String {
    let h = h.trim().trim_end_matches('/');
    h.strip_prefix("https://").unwrap_or(h).to_string()
}

pub fn base_url(host: &str) -> String {
    if host.contains("://") {
        host.to_string()
    } else {
        format!("https://{host}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hosts() {
        assert_eq!(normalize_host("https://cle.inserm.fr/"), "cle.inserm.fr");
        assert_eq!(base_url("my.labguru.com"), "https://my.labguru.com");
        assert_eq!(base_url("http://127.0.0.1:9"), "http://127.0.0.1:9");
    }

    #[test]
    fn roundtrip() {
        let mut c = Config::default();
        c.hosts.entry("h".into()).or_default().users.insert(
            "a@b".into(),
            User {
                account_id: Some(3),
                token: None,
            },
        );
        let s = toml::to_string_pretty(&c).unwrap();
        assert_eq!(toml::from_str::<Config>(&s).unwrap(), c);
        assert_eq!(c.setting("prompt"), "enabled");
    }
}
