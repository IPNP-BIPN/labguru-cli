//! lg auth: tokens in the system keyring (else config file, mode 600); LABGURU_TOKEN / LABGURU_HOST win.

use crate::api::{self, AuthRequired, Client};
use crate::config::{Config, User, normalize_host};
use crate::output::can_prompt;
use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use std::io::Read;

pub const DEFAULT_HOST: &str = "my.labguru.com";

fn service(host: &str) -> String {
    format!("lg:{host}")
}

fn keyring_entry(host: &str, user: &str) -> Option<keyring::Entry> {
    if std::env::var_os("LG_NO_KEYRING").is_some() {
        return None;
    }
    keyring::Entry::new(&service(host), user).ok()
}

/// Host to use: --hostname, else LABGURU_HOST, else the active host, else the only one, else my.labguru.com.
pub fn pick_host(cfg: &Config, flag: Option<&str>) -> String {
    if let Some(h) = flag.filter(|h| !h.is_empty()) {
        return normalize_host(h);
    }
    if let Ok(h) = std::env::var("LABGURU_HOST")
        && !h.is_empty()
    {
        return normalize_host(&h);
    }
    cfg.current_host
        .clone()
        .or_else(|| (cfg.hosts.len() == 1).then(|| cfg.hosts.keys().next().cloned()).flatten())
        .unwrap_or_else(|| DEFAULT_HOST.to_string())
}

pub enum Source {
    Env,
    Keyring,
    File,
}

impl Source {
    fn label(&self) -> &'static str {
        match self {
            Source::Env => "LABGURU_TOKEN",
            Source::Keyring => tr!("system keyring", "trousseau système"),
            Source::File => tr!("config file", "fichier de config"),
        }
    }
}

fn stored_token(cfg: &Config, host: &str, user: &str) -> Option<(String, Source)> {
    if let Some(t) = keyring_entry(host, user).and_then(|e| e.get_password().ok()) {
        return Some((t, Source::Keyring));
    }
    cfg.hosts.get(host)?.users.get(user)?.token.clone().map(|t| (t, Source::File))
}

pub fn token_for(cfg: &Config, host: &str) -> Option<(String, Source)> {
    if let Ok(t) = std::env::var("LABGURU_TOKEN")
        && !t.trim().is_empty()
    {
        return Some((t.trim().to_string(), Source::Env));
    }
    let h = cfg.hosts.get(host)?;
    let user = h.current_user.clone().or_else(|| h.users.keys().next().cloned())?;
    stored_token(cfg, host, &user)
}

/// An API client for the chosen host, or exit code 4 with what to do.
pub fn client(flag: Option<&str>) -> Result<Client> {
    let mut cfg = Config::load()?;
    let host = pick_host(&cfg, flag);
    let Some((token, _)) = token_for(&cfg, &host) else {
        return Err(AuthRequired(tr!(
            format!("Not logged in to {host}. Run: lg auth login --hostname {host}  (or set LABGURU_TOKEN)"),
            format!("Pas connecté à {host}. Lancez : lg auth login --hostname {host}  (ou définissez LABGURU_TOKEN)")
        ))
        .into());
    };
    let mut c = Client::new(&host, &token);
    c.caps = cfg.hosts.get(&host).and_then(|h| h.capabilities.clone());
    if c.caps.is_none() && std::env::var_os("LG_NO_CAPS").is_none() {
        let caps = api::detect_caps(&host);
        cfg.hosts.entry(host.clone()).or_default().capabilities = Some(caps.clone());
        if cfg.hosts.get(&host).is_some_and(|h| !h.users.is_empty()) {
            let _ = cfg.save();
        }
        c.caps = Some(caps);
    }
    Ok(c)
}

fn read_line(prompt: &str) -> Result<String> {
    Ok(dialoguer::Input::<String>::new().with_prompt(prompt).interact_text()?)
}

/// POST /sessions with e-mail + password; the password is never stored.
pub fn token_from_password(host: &str, login: &str, password: &str, account: Option<i64>) -> Result<String> {
    let url = format!("{}{}", crate::config::base_url(host), api::api_path("sessions"));
    let mut body = json!({"login": login, "password": password});
    if let Some(a) = account {
        body["account_id"] = json!(a);
    }
    let mut r = api::agent(60)
        .post(&url)
        .header("Content-Type", "application/json")
        .send(serde_json::to_string(&body)?)
        .map_err(|e| anyhow::anyhow!("POST /api/v1/sessions: {e}"))?;
    let status = r.status().as_u16();
    let text = r.body_mut().read_to_string().unwrap_or_default();
    let v: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
    match v.get("token").and_then(Value::as_str) {
        Some(t) if (200..300).contains(&status) && !t.is_empty() && t != "-1" => Ok(t.to_string()),
        _ if status == 401 || (200..300).contains(&status) => Err(AuthRequired(tr!(
            format!("{host} refused this e-mail and password. If your institution uses SSO, create a token in Labguru (Account > API token) and run: lg auth login --hostname {host} --with-token"),
            format!("{host} a refusé cet e-mail et ce mot de passe. Si votre établissement utilise le SSO, créez un token dans Labguru (Compte > API token) puis : lg auth login --hostname {host} --with-token")
        ))
        .into()),
        _ => bail!("POST /api/v1/sessions: HTTP {status}"),
    }
}

pub struct LoginArgs {
    pub host: Option<String>,
    pub with_token: bool,
    pub user: Option<String>,
    pub account: Option<i64>,
}

pub fn login(a: LoginArgs) -> Result<()> {
    let mut cfg = Config::load()?;
    let host = match a.host.clone() {
        Some(h) => normalize_host(&h),
        None if can_prompt() && !a.with_token => {
            let def = pick_host(&cfg, None);
            normalize_host(
                &dialoguer::Input::<String>::new()
                    .with_prompt(tr!("Labguru host", "Hôte Labguru"))
                    .default(def)
                    .interact_text()?,
            )
        }
        None => pick_host(&cfg, None),
    };
    let (token, user) = if a.with_token {
        let mut s = String::new();
        std::io::stdin().read_to_string(&mut s)?;
        let t = s.trim().to_string();
        if t.is_empty() {
            bail!(tr!(
                "No token on standard input. Example: lg auth login --with-token < token.txt",
                "Aucun token sur l'entrée standard. Exemple : lg auth login --with-token < token.txt"
            ));
        }
        if t.contains(char::is_whitespace) {
            bail!(tr!(
                "Standard input holds several words or lines, not a token. Copy only the token, then: pbpaste | lg auth login --with-token",
                "L'entrée standard contient plusieurs mots ou lignes, pas un token. Copiez seulement le token, puis : pbpaste | lg auth login --with-token"
            ));
        }
        (t, a.user.clone().unwrap_or_else(|| "default".into()))
    } else {
        if !can_prompt() {
            bail!(tr!(
                "lg auth login needs a terminal to ask for your e-mail and password; otherwise use --with-token (token on standard input)",
                "lg auth login a besoin d'un terminal pour demander e-mail et mot de passe ; sinon utilisez --with-token (token sur l'entrée standard)"
            ));
        }
        let login = match &a.user {
            Some(u) => u.clone(),
            None => read_line(tr!("E-mail", "E-mail"))?,
        };
        let pw = dialoguer::Password::new()
            .with_prompt(tr!("Password (not stored)", "Mot de passe (non stocké)"))
            .interact()?;
        let t = token_from_password(&host, &login, &pw, a.account)?;
        (t, login)
    };
    api::register_secret(&token);
    let user = match a.account {
        Some(acc) if !user.contains('#') => format!("{user}#{acc}"),
        _ => user,
    };
    // check the token before saving it
    let mut c = Client::new(&host, &token);
    c.waits = vec![5];
    c.get_q("projects", &[("page".into(), "1".into())])
        .context(tr!("the token does not work", "le token ne fonctionne pas"))?;

    let caps = api::detect_caps(&host);
    let stored_in_keyring = keyring_entry(&host, &user).is_some_and(|e| e.set_password(&token).is_ok());
    let h = cfg.hosts.entry(host.clone()).or_default();
    h.users.insert(
        user.clone(),
        User {
            account_id: a.account,
            token: (!stored_in_keyring).then(|| token.clone()),
        },
    );
    h.current_user = Some(user.clone());
    h.capabilities = Some(caps.clone());
    cfg.current_host = Some(host.clone());
    cfg.save()?;
    eprintln!(
        "✓ {}",
        tr!(
            format!("Logged in to {host} as {user}"),
            format!("Connecté à {host} en tant que {user}")
        )
    );
    if !stored_in_keyring {
        eprintln!(
            "! {}",
            tr!(
                format!(
                    "System keyring unavailable: token saved in {} (mode 600)",
                    crate::config::path().display()
                ),
                format!(
                    "Trousseau système indisponible : token rangé dans {} (mode 600)",
                    crate::config::path().display()
                )
            )
        );
    }
    eprintln!("  {}", caps_summary(&caps));
    Ok(())
}

pub fn caps_summary(c: &crate::config::Caps) -> String {
    let api = if c.v2 { "API v1 + v2" } else { tr!("API v1 only", "API v1 seule") };
    if c.endpoints.is_empty() {
        tr!(
            format!("{api}; endpoint list unavailable (all commands allowed)"),
            format!("{api} ; liste des endpoints indisponible (toutes les commandes autorisées)")
        )
    } else {
        tr!(
            format!("{api}, {} documented endpoints", c.endpoints.len()),
            format!("{api}, {} endpoints documentés", c.endpoints.len())
        )
    }
}

pub fn status(flag: Option<&str>) -> Result<i32> {
    let cfg = Config::load()?;
    let mut hosts: Vec<String> = cfg.hosts.keys().cloned().collect();
    let env_host = std::env::var("LABGURU_HOST")
        .ok()
        .filter(|h| !h.is_empty())
        .map(|h| normalize_host(&h));
    if let Some(h) = flag.map(normalize_host).or(env_host) {
        hosts.retain(|x| *x == h);
        if hosts.is_empty() {
            hosts.push(h);
        }
    }
    if hosts.is_empty() && std::env::var_os("LABGURU_TOKEN").is_some() {
        hosts.push(pick_host(&cfg, flag));
    }
    if hosts.is_empty() {
        return Err(AuthRequired(tr!("Not logged in. Run: lg auth login", "Pas connecté. Lancez : lg auth login").into()).into());
    }
    let mut bad = false;
    for host in hosts {
        println!("{host}");
        let h = cfg.hosts.get(&host).cloned().unwrap_or_default();
        let mut users: Vec<(String, Option<(String, Source)>)> =
            h.users.keys().map(|u| (u.clone(), stored_token(&cfg, &host, u))).collect();
        if let Ok(t) = std::env::var("LABGURU_TOKEN")
            && !t.trim().is_empty()
        {
            users.insert(0, ("LABGURU_TOKEN".into(), Some((t.trim().to_string(), Source::Env))));
        }
        for (u, tok) in users {
            let active = h.current_user.as_deref() == Some(u.as_str()) || u == "LABGURU_TOKEN";
            let mark = if active { "*" } else { " " };
            match tok {
                None => {
                    bad = true;
                    println!(
                        "  {mark} {u}: {}",
                        tr!("no token found; run lg auth login", "aucun token trouvé ; lancez lg auth login")
                    );
                }
                Some((t, src)) => {
                    let mut c = Client::new(&host, &t);
                    c.waits = vec![];
                    let ok = c.get_q("projects", &[("page".into(), "1".into())]);
                    let state = match &ok {
                        Ok(_) => tr!("valid token", "token valide").to_string(),
                        Err(e) => {
                            bad = true;
                            format!("{} ({})", tr!("token not accepted", "token refusé"), api::redact(&e.to_string()))
                        }
                    };
                    println!("  {mark} {u} — {state} — {}", src.label());
                }
            }
        }
        if let Some(c) = &h.capabilities {
            println!("    {}", caps_summary(c));
        }
    }
    Ok(if bad { 1 } else { 0 })
}

pub fn logout(flag: Option<&str>, user: Option<String>) -> Result<()> {
    let mut cfg = Config::load()?;
    let host = pick_host(&cfg, flag);
    let Some(h) = cfg.hosts.get_mut(&host) else {
        bail!(tr!(format!("Not logged in to {host}"), format!("Pas connecté à {host}")));
    };
    let user = match user.or(h.current_user.clone()) {
        Some(u) => u,
        None => bail!(tr!("Which account? Use --user", "Quel compte ? Utilisez --user")),
    };
    if h.users.remove(&user).is_none() {
        bail!(tr!(
            format!("{user} is not logged in to {host} (see lg auth status)"),
            format!("{user} n'est pas connecté à {host} (voir lg auth status)")
        ));
    }
    if let Some(e) = keyring_entry(&host, &user) {
        let _ = e.delete_credential();
    }
    h.current_user = h.users.keys().next().cloned();
    if h.users.is_empty() {
        cfg.hosts.remove(&host);
        if cfg.current_host.as_deref() == Some(host.as_str()) {
            cfg.current_host = cfg.hosts.keys().next().cloned();
        }
    }
    cfg.save()?;
    eprintln!(
        "✓ {}",
        tr!(format!("Logged out of {host} ({user})"), format!("Déconnecté de {host} ({user})"))
    );
    Ok(())
}

pub fn switch(flag: Option<&str>, user: Option<String>) -> Result<()> {
    let mut cfg = Config::load()?;
    let choices: Vec<(String, String)> = cfg
        .hosts
        .iter()
        .flat_map(|(h, x)| x.users.keys().map(move |u| (h.clone(), u.clone())))
        .collect();
    let flag = flag.map(normalize_host);
    let mut cands: Vec<&(String, String)> = choices
        .iter()
        .filter(|(h, u)| flag.as_ref().is_none_or(|f| f == h) && user.as_ref().is_none_or(|x| x == u))
        .collect();
    let current =
        |h: &str, u: &str| cfg.current_host.as_deref() == Some(h) && cfg.hosts.get(h).and_then(|x| x.current_user.as_deref()) == Some(u);
    if user.is_none() && cands.len() > 1 {
        // like gh: two accounts → switch to the other one; more → ask
        let others: Vec<&(String, String)> = cands.iter().copied().filter(|(h, u)| !current(h, u)).collect();
        if others.len() == 1 {
            cands = others;
        } else if can_prompt() {
            let items: Vec<String> = cands.iter().map(|(h, u)| format!("{h}  {u}")).collect();
            let i = dialoguer::Select::new()
                .with_prompt(tr!("Account", "Compte"))
                .items(&items)
                .default(0)
                .interact()?;
            cands = vec![cands[i]];
        } else {
            bail!(tr!(
                "Several accounts: use --hostname and --user",
                "Plusieurs comptes : utilisez --hostname et --user"
            ));
        }
    }
    let Some((h, u)) = cands.first().map(|x| (x.0.clone(), x.1.clone())) else {
        bail!(tr!(
            "No matching account; see lg auth status",
            "Aucun compte correspondant ; voir lg auth status"
        ));
    };
    cfg.current_host = Some(h.clone());
    cfg.hosts.get_mut(&h).unwrap().current_user = Some(u.clone());
    cfg.save()?;
    eprintln!("✓ {}", tr!(format!("Now using {u} on {h}"), format!("Compte actif : {u} sur {h}")));
    Ok(())
}
