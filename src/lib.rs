//! lg: a Labguru command-line client, in the spirit of gh.

#[macro_use]
pub mod i18n;
pub mod api;
pub mod auth;
pub mod bib;
pub mod cli;
pub mod cmd;
pub mod config;
pub mod labguru;
pub mod md;
pub mod notebook;
pub mod output;

use anyhow::Result;
use clap::{CommandFactory, FromArgMatches};
use cli::*;
use std::ffi::OsString;

/// Splits an alias expansion like a shell would (quotes, no variables).
pub fn shell_split(s: &str) -> Vec<String> {
    let (mut out, mut cur, mut quote, mut any) = (Vec::new(), String::new(), None, false);
    for c in s.chars() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some(_), c) => cur.push(c),
            (None, '\'' | '"') => {
                quote = Some(c);
                any = true;
            }
            (None, c) if c.is_whitespace() => {
                if any || !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
                any = false;
            }
            (None, c) => cur.push(c),
        }
    }
    if any || !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// `lg <alias> args…` → `lg <expansion> args…` (built-in commands always win).
pub fn expand_alias(args: Vec<OsString>, aliases: &std::collections::BTreeMap<String, String>) -> Vec<OsString> {
    let Some(first) = args.get(1).and_then(|a| a.to_str()) else {
        return args;
    };
    let builtin = Cli::command()
        .get_subcommands()
        .any(|c| c.get_name() == first || c.get_all_aliases().any(|a| a == first));
    match aliases.get(first) {
        Some(exp) if !builtin => {
            let mut v = vec![args[0].clone()];
            v.extend(shell_split(exp).into_iter().map(OsString::from));
            v.extend(args[2..].iter().cloned());
            v
        }
        _ => args,
    }
}

pub fn command() -> clap::Command {
    i18n::localize(Cli::command())
}

/// Exit codes: 0 success, 1 error, 2 usage, 4 authentication required.
pub fn run() -> i32 {
    // `lg … | head`: a closed pipe ends quietly instead of panicking
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let msg = info
            .payload()
            .downcast_ref::<String>()
            .map(String::as_str)
            .or(info.payload().downcast_ref::<&str>().copied())
            .unwrap_or("");
        if msg.contains("Broken pipe") || msg.contains("os error 32") {
            std::process::exit(0);
        }
        hook(info)
    }));
    let cfg = config::Config::load().unwrap_or_default();
    let args = expand_alias(std::env::args_os().collect(), &cfg.aliases);
    let matches = match command().try_get_matches_from(args) {
        Ok(m) => m,
        Err(e) => {
            let _ = e.print();
            return if e.use_stderr() { 2 } else { 0 };
        }
    };
    let cli = match Cli::from_arg_matches(&matches) {
        Ok(c) => c,
        Err(e) => {
            let _ = e.print();
            return 2;
        }
    };
    match dispatch(cli) {
        Ok(code) => code,
        Err(e) => {
            eprintln!(
                "{} {}",
                if output::color() { "\x1b[31mX\x1b[0m" } else { "X" },
                api::redact(&format!("{e:#}"))
            );
            if e.downcast_ref::<api::AuthRequired>().is_some() {
                4
            } else if e.downcast_ref::<cmd::Usage>().is_some() {
                2
            } else {
                1
            }
        }
    }
}

fn dispatch(cli: Cli) -> Result<i32> {
    let host = cli.hostname.as_deref();
    let client = || auth::client(host);
    match cli.cmd {
        Cmd::Auth(a) => match a {
            AuthCmd::Login { with_token, user, account } => auth::login(auth::LoginArgs {
                host: cli.hostname.clone(),
                with_token,
                user,
                account,
            })?,
            AuthCmd::Status => return auth::status(host),
            AuthCmd::Logout { user } => auth::logout(host, user)?,
            AuthCmd::Switch { user } => auth::switch(host, user)?,
        },
        Cmd::Project(c) => cmd::project(&client()?, c)?,
        Cmd::Folder(c) => cmd::folder(&client()?, c)?,
        Cmd::Experiment(c @ (ExperimentCmd::Flag { .. } | ExperimentCmd::Sign { .. })) => cmd::explain_limits(client().ok().as_ref(), c)?,
        Cmd::Experiment(c) => cmd::experiment(&client()?, c)?,
        Cmd::Section(c) => cmd::section(&client()?, c)?,
        Cmd::Note(c) => cmd::note(&client()?, c)?,
        Cmd::Paper(c) => cmd::paper(&client()?, c)?,
        Cmd::Attachment(c) => cmd::attachment(&client()?, c)?,
        Cmd::Tag(c) => cmd::tag(&client()?, c)?,
        Cmd::Browse { kind, id } => cmd::browse(&client()?, kind, id)?,
        Cmd::Api(a) => cmd::api(&client()?, a)?,
        Cmd::Alias(a) => alias(a)?,
        Cmd::Config(c) => config_cmd(c)?,
        Cmd::Notebook(n) => return notebook_cmd(host, n),
        Cmd::Completion { shell } => clap_complete::generate(shell, &mut command(), "lg", &mut std::io::stdout()),
        Cmd::Man { output } => {
            let man = clap_mangen::Man::new(command());
            match output {
                Some(d) => {
                    std::fs::create_dir_all(&d)?;
                    let mut f = std::fs::File::create(d.join("lg.1"))?;
                    man.render(&mut f)?;
                }
                None => man.render(&mut std::io::stdout())?,
            }
        }
    }
    Ok(0)
}

fn alias(a: AliasCmd) -> Result<()> {
    let mut cfg = config::Config::load()?;
    match a {
        AliasCmd::Set { name, expansion } => {
            if Cli::command().get_subcommands().any(|c| c.get_name() == name) {
                return Err(cmd::Usage(tr!(
                    format!("{name} is a built-in command"),
                    format!("{name} est une commande intégrée")
                ))
                .into());
            }
            let first = shell_split(&expansion).into_iter().next().unwrap_or_default();
            if !Cli::command().get_subcommands().any(|c| c.get_name() == first) {
                return Err(cmd::Usage(tr!(
                    format!("the expansion must start with an lg command, not '{first}'"),
                    format!("le raccourci doit commencer par une commande lg, pas « {first} »")
                ))
                .into());
            }
            cfg.aliases.insert(name.clone(), expansion);
            cfg.save()?;
            eprintln!("✓ {}", tr!(format!("Added alias {name}"), format!("Raccourci {name} ajouté")));
        }
        AliasCmd::List => {
            for (k, v) in &cfg.aliases {
                println!("{k}\t{v}");
            }
        }
        AliasCmd::Delete { name } => {
            if cfg.aliases.remove(&name).is_none() {
                anyhow::bail!(tr!(
                    format!("no alias {name} (see lg alias list)"),
                    format!("pas de raccourci {name} (voir lg alias list)")
                ));
            }
            cfg.save()?;
            eprintln!("✓ {}", tr!(format!("Deleted alias {name}"), format!("Raccourci {name} supprimé")));
        }
    }
    Ok(())
}

fn config_cmd(c: ConfigCmd) -> Result<()> {
    let mut cfg = config::Config::load()?;
    let known = |k: &str| -> Result<()> {
        if config::SETTINGS.iter().any(|s| s.0 == k) {
            return Ok(());
        }
        let names: Vec<&str> = config::SETTINGS.iter().map(|s| s.0).collect();
        Err(cmd::Usage(tr!(
            format!("unknown setting {k}; known: {}", names.join(", ")),
            format!("réglage inconnu {k} ; réglages : {}", names.join(", "))
        ))
        .into())
    };
    match c {
        ConfigCmd::Get { key } => {
            known(&key)?;
            println!("{}", cfg.setting(&key));
        }
        ConfigCmd::Set { key, value } => {
            known(&key)?;
            if key == "prompt" && !["enabled", "disabled"].contains(&value.as_str()) {
                return Err(cmd::Usage("prompt: enabled | disabled".into()).into());
            }
            cfg.settings.insert(key, value);
            cfg.save()?;
        }
        ConfigCmd::List => {
            for (k, _, help) in config::SETTINGS {
                println!("{k}={}\t# {help}", cfg.setting(k));
            }
        }
    }
    Ok(())
}

fn notebook_cmd(host: Option<&str>, n: NotebookCmd) -> Result<i32> {
    use notebook::*;
    let opts = |project| PushOpts {
        apply: false,
        project,
        today: jiff::Zoned::now().date(),
        tz: jiff::tz::TimeZone::system(),
    };
    let client_for = |root: &std::path::Path| -> Result<api::Client> {
        let h = host.map(String::from).or_else(|| load_config(root).ok().and_then(|c| c.host));
        auth::client(h.as_deref())
    };
    match n {
        NotebookCmd::Init { dir, project } => {
            let cfg = config::Config::load()?;
            init(&dir, project, host.map(String::from).or(cfg.current_host))?;
        }
        NotebookCmd::New { folder, title, root } => {
            let root = find_root(root.as_deref())?;
            let p = new_entry(&root, &folder, &title)?;
            println!("{}", p.display());
        }
        NotebookCmd::Push { apply, project, root } => {
            let root = find_root(root.as_deref())?;
            let c = client_for(&root)?;
            let r = push(&c, &root, PushOpts { apply, ..opts(project) }, &mut |l| println!("{l}"))?;
            if r.changes == 0 {
                eprintln!("✓ {}", tr!("Labguru is up to date.", "Labguru est à jour."));
            } else if apply {
                eprintln!(
                    "✓ {}",
                    tr!(
                        format!("{} changes written to {}.", r.changes, c.host),
                        format!("{} changements écrits dans {}.", r.changes, c.host)
                    )
                );
            } else {
                eprintln!(
                    "{}",
                    tr!(
                        format!("Dry run: {} changes, nothing written. Run again with --apply.", r.changes),
                        format!("À blanc : {} changements, rien n'a été écrit. Relancez avec --apply.", r.changes)
                    )
                );
            }
        }
        NotebookCmd::Check { project, root } => {
            let root = find_root(root.as_deref())?;
            let c = client_for(&root)?;
            let r = check(&c, &root, opts(project), &mut |l| println!("{l}"))?;
            if r.changes == 0 {
                eprintln!("✓ {}", tr!("Labguru matches the notebook.", "Labguru correspond au cahier."));
            } else {
                eprintln!(
                    "{}",
                    tr!(
                        format!("{} differences (lg notebook push --apply fixes them).", r.changes),
                        format!("{} écarts (lg notebook push --apply les corrige).", r.changes)
                    )
                );
                return Ok(1);
            }
        }
    }
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn split() {
        assert_eq!(
            shell_split(r#"experiment list --project 12 -t "{{.id}} x" ''"#),
            ["experiment", "list", "--project", "12", "-t", "{{.id}} x", ""]
        );
    }

    #[test]
    fn aliases() {
        let mut a = std::collections::BTreeMap::new();
        a.insert("el".to_string(), "experiment list --project 3".to_string());
        a.insert("project".to_string(), "note list".to_string());
        let args = |v: &[&str]| v.iter().map(OsString::from).collect::<Vec<_>>();
        assert_eq!(
            expand_alias(args(&["lg", "el", "-L", "2"]), &a),
            args(&["lg", "experiment", "list", "--project", "3", "-L", "2"])
        );
        assert_eq!(expand_alias(args(&["lg", "project", "list"]), &a), args(&["lg", "project", "list"]));
    }

    #[test]
    fn cli_parses() {
        Cli::command().debug_assert();
        let p = |v: &[&str]| Cli::try_parse_from(v);
        assert!(
            p(&[
                "lg",
                "experiment",
                "create",
                "--project",
                "1",
                "--folder",
                "2",
                "--title",
                "x",
                "--start",
                "2026-01-01"
            ])
            .is_ok()
        );
        assert!(
            p(&[
                "lg",
                "folder",
                "create",
                "--project",
                "1",
                "--title",
                "x",
                "--status",
                "in-progress"
            ])
            .is_ok()
        );
        assert!(p(&["lg", "section", "create", "--name", "R"]).is_err()); // a target is required
        assert!(p(&["lg", "tag", "create", "x", "--experiment", "1", "--project", "2"]).is_err()); // only one target
        let api = p(&["lg", "api", "-X", "PUT", "experiments/1", "-f", "item[title]=a", "--jq", ".id"]).unwrap();
        assert!(matches!(api.cmd, Cmd::Api(a) if a.method.as_deref() == Some("PUT") && a.fields == ["item[title]=a"]));
        let e = p(&["lg", "nope"]).unwrap_err();
        assert!(e.use_stderr());
    }

    #[test]
    fn french_help_is_complete() {
        // every translated text must exist in the command tree (catches typos and stale entries)
        fn texts(c: &clap::Command, out: &mut Vec<String>) {
            if let Some(a) = c.get_about() {
                out.push(a.to_string());
            }
            for a in c.get_arguments() {
                if let Some(h) = a.get_help() {
                    out.push(h.to_string());
                }
            }
            for s in c.get_subcommands() {
                texts(s, out);
            }
        }
        let mut all = Vec::new();
        let mut built = Cli::command();
        built.build();
        texts(&built, &mut all);
        for (en, _) in i18n::HELP_FR {
            assert!(all.iter().any(|t| t == en), "unused translation: {en}");
        }
        let helps: Vec<String> = {
            fn after(c: &clap::Command, out: &mut Vec<String>) {
                out.extend(c.get_after_help().map(|h| h.to_string()));
                c.get_subcommands().for_each(|s| after(s, out));
            }
            let mut v = Vec::new();
            after(&built, &mut v);
            v
        };
        for (en, _) in i18n::PROSE_FR {
            assert!(helps.iter().any(|h| h.contains(en)), "unused prose translation: {en}");
        }
        let fr = i18n::localize_with(Cli::command());
        assert_eq!(fr.get_about().unwrap().to_string(), "Travailler avec Labguru en ligne de commande");
        assert!(fr.get_after_help().unwrap().to_string().starts_with("Exemples :"));
    }
}
