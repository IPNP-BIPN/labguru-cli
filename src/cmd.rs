//! Object commands: project, folder, experiment, section, note, paper, attachment, tag, browse, api.

use crate::api::Client;
use crate::cli::*;
use crate::labguru::{self as lgu, id, s};
use crate::output::{OutArgs, can_prompt, print_list, print_object};
use crate::{bib, md};
use anyhow::{Context, Result, bail};
use serde_json::{Map, Value, json};
use std::io::{IsTerminal, Read};

/// Usage errors (exit code 2).
#[derive(Debug)]
pub struct Usage(pub String);
impl std::fmt::Display for Usage {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for Usage {}

struct K {
    name: &'static str,
    path: &'static str,
    web: &'static str,
    cols: &'static [(&'static str, &'static str)],
    view: &'static [(&'static str, &'static str)],
}

const PROJECT: K = K {
    name: "project",
    path: "projects",
    web: "knowledge/projects",
    cols: &[("ID", "id"), ("TITLE", "title"), ("START", "start_date"), ("UPDATED", "updated_at")],
    view: &[
        ("Title", "title"),
        ("ID", "id"),
        ("Start", "start_date"),
        ("Owner", "owner.name"),
        ("Updated", "updated_at"),
        ("UUID", "uuid"),
    ],
};
const FOLDER: K = K {
    name: "folder",
    path: "milestones",
    web: "knowledge/milestones",
    cols: &[
        ("ID", "id"),
        ("TITLE", "title"),
        ("STATUS", "status"),
        ("PROJECT", "project_id"),
        ("START", "start_date"),
    ],
    view: &[
        ("Title", "title"),
        ("ID", "id"),
        ("Status", "status"),
        ("Project", "project_id"),
        ("Parent", "ancestry"),
        ("Start", "start_date"),
        ("UUID", "uuid"),
    ],
};
const EXPERIMENT: K = K {
    name: "experiment",
    path: "experiments",
    web: "knowledge/experiments",
    cols: &[
        ("ID", "id"),
        ("TITLE", "title"),
        ("PROJECT", "project_id"),
        ("FOLDER", "milestone_id"),
        ("START", "start_date"),
        ("END", "end_date"),
    ],
    view: &[
        ("Title", "title"),
        ("ID", "id"),
        ("Project", "project_id"),
        ("Folder", "milestone_id"),
        ("Start", "start_date"),
        ("End", "end_date"),
        ("Duration", "duration"),
        ("Owner", "owner.name"),
        ("UUID", "uuid"),
    ],
};
const NOTE: K = K {
    name: "note",
    path: "notes",
    web: "knowledge/notes",
    cols: &[
        ("ID", "id"),
        ("TITLE", "title"),
        ("PROJECT", "project_id"),
        ("UPDATED", "updated_at"),
    ],
    view: &[
        ("Title", "title"),
        ("ID", "id"),
        ("Project", "project_id"),
        ("Updated", "updated_at"),
    ],
};
const PAPER: K = K {
    name: "paper",
    path: "papers",
    web: "knowledge/papers",
    cols: &[
        ("ID", "id"),
        ("TITLE", "name"),
        ("JOURNAL", "journal"),
        ("DATE", "publication_date"),
    ],
    view: &[
        ("Title", "name"),
        ("ID", "id"),
        ("Authors", "authors"),
        ("Journal", "journal"),
        ("Date", "publication_date"),
        ("DOI", "doi"),
        ("URL", "url"),
    ],
};

fn kind(k: Kind) -> &'static K {
    match k {
        Kind::Project => &PROJECT,
        Kind::Folder => &FOLDER,
        Kind::Experiment => &EXPERIMENT,
        Kind::Note => &NOTE,
        Kind::Paper => &PAPER,
    }
}

pub fn web_url(c: &Client, web: &str, id: Option<u64>) -> String {
    match id {
        Some(i) => format!("{}/{web}/{i}", c.base),
        None => format!("{}/{web}", c.base),
    }
}

/// Opens an object fetched from the API at its web page (`url` field), or says it has none.
fn open_obj(c: &Client, obj: &Value) -> Result<()> {
    match s(obj, "url") {
        u if u.starts_with('/') => open_url(&format!("{}{u}", c.base)),
        _ => Err(Usage(tr!("this object has no web page of its own", "cet objet n'a pas de page web à lui").into()).into()),
    }
}

/// The object's own web page, read from its `url` field (a paper's `url` is its DOI, hence the fallback).
fn object_url(c: &Client, k: &K, id: u64) -> Result<String> {
    let obj = c.get(&format!("{}/{id}", k.path))?;
    Ok(match obj["url"].as_str() {
        Some(u) if u.starts_with('/') => format!("{}{u}", c.base),
        _ => web_url(c, k.web, Some(id)),
    })
}

pub fn open_url(url: &str) -> Result<()> {
    let cfg = crate::config::Config::load().unwrap_or_default();
    let browser = std::env::var("BROWSER")
        .ok()
        .filter(|b| !b.is_empty())
        .unwrap_or_else(|| cfg.setting("browser"));
    eprintln!("{} {url}", tr!("Opening", "Ouverture de"));
    let status = if !browser.is_empty() {
        std::process::Command::new(&browser).arg(url).status()
    } else if cfg!(target_os = "macos") {
        std::process::Command::new("open").arg(url).status()
    } else if cfg!(windows) {
        std::process::Command::new("cmd").args(["/C", "start", "", url]).status()
    } else {
        std::process::Command::new("xdg-open").arg(url).status()
    };
    match status {
        Ok(s) if s.success() => Ok(()),
        _ => bail!(tr!(
            format!("Could not open a browser. Open this URL yourself: {url} (or set: lg config set browser <command>)"),
            format!("Impossible d'ouvrir un navigateur. Ouvrez vous-même : {url} (ou réglez : lg config set browser <commande>)")
        )),
    }
}

fn confirm(what: &str, yes: bool) -> Result<()> {
    if yes {
        return Ok(());
    }
    if !can_prompt() {
        return Err(Usage(tr!(
            format!("{what}: add --yes to confirm (no terminal to ask)"),
            format!("{what} : ajoutez --yes pour confirmer (pas de terminal pour demander)")
        ))
        .into());
    }
    let ok = dialoguer::Confirm::new()
        .with_prompt(format!("{what} ?"))
        .default(false)
        .interact()?;
    if !ok {
        bail!(tr!("Cancelled", "Annulé"));
    }
    Ok(())
}

fn date_or_err(s: &str) -> Result<String> {
    s.parse::<jiff::civil::Date>().map(|d| d.to_string()).map_err(|_| {
        Usage(tr!(
            format!("invalid date '{s}': use YYYY-MM-DD"),
            format!("date invalide « {s} » : utilisez AAAA-MM-JJ")
        ))
        .into()
    })
}

/// --body, --body-file (or '-'), or standard input when it is not a terminal.
pub fn read_body(b: &BodyArgs) -> Result<Option<String>> {
    if let Some(t) = &b.body {
        return Ok(Some(t.clone()));
    }
    let mut s = String::new();
    match &b.body_file {
        Some(p) if p.as_os_str() == "-" => {
            std::io::stdin().read_to_string(&mut s)?;
        }
        Some(p) => s = std::fs::read_to_string(p).with_context(|| p.display().to_string())?,
        None if !std::io::stdin().is_terminal() => {
            std::io::stdin().read_to_string(&mut s)?;
            if s.trim().is_empty() {
                return Ok(None);
            }
        }
        None => return Ok(None),
    }
    Ok(Some(s))
}

fn html(mdtext: &str) -> String {
    md::to_html(mdtext, &mut |_| None)
}

fn list_generic(c: &Client, k: &K, l: &ListArgs, keep: &dyn Fn(&Value) -> bool) -> Result<()> {
    if l.web {
        return open_url(&web_url(c, k.web, None));
    }
    let mut rows: Vec<Value> = c.all(k.path, &[])?.into_iter().filter(|r| keep(r)).collect();
    rows.truncate(l.limit);
    print_list(&rows, k.cols, &l.out)
}

fn view_generic(c: &Client, k: &K, v: &ViewArgs) -> Result<()> {
    if v.web {
        return open_url(&object_url(c, k, v.id)?);
    }
    let obj = c.get(&format!("{}/{}", k.path, v.id))?;
    print_object(&obj, k.view, &v.out)?;
    if !v.out.is_json() {
        print_sections(&obj)?;
    }
    Ok(())
}

fn print_sections(obj: &Value) -> Result<()> {
    let tty = crate::output::stdout_tty();
    for sec in lgu::sections(obj) {
        let text: Vec<String> = sec
            .get("elements")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|e| e.get("data").and_then(Value::as_str))
                    .map(md::html_to_text)
                    .filter(|t| !t.is_empty())
                    .collect()
            })
            .unwrap_or_default();
        if tty {
            println!("\n## {}\n{}", s(&sec, "name"), text.join("\n"));
        } else {
            println!("section\t{}\t{}", s(&sec, "name"), text.join(" ").replace(['\n', '\t'], " "));
        }
    }
    for k in ["description", "body"] {
        if let Some(d) = obj.get(k).and_then(Value::as_str).filter(|d| !d.trim().is_empty())
            && tty
        {
            println!("\n{}", md::html_to_text(d));
        }
    }
    Ok(())
}

fn delete_generic(c: &Client, k: &K, d: &DeleteArgs, path: &str) -> Result<()> {
    // the /api-docs omit most v1 DELETEs that work (milestones on cle.inserm.fr): only a missing v2 is a sure refusal
    if path.starts_with("v2/") {
        c.require("DELETE", path, &format!("lg {} delete", k.name))?;
    }
    confirm(
        &tr!(format!("Delete {} {}", k.name, d.id), format!("Supprimer {} {}", k.name, d.id)),
        d.yes,
    )?;
    c.delete(path)?;
    eprintln!(
        "✓ {}",
        tr!(format!("Deleted {} {}", k.name, d.id), format!("{} {} supprimé", k.name, d.id))
    );
    Ok(())
}

fn created(k: &K, v: &Value) {
    eprintln!(
        "✓ {}",
        tr!(
            format!("Created {} {}", k.name, id(v).unwrap_or(0)),
            format!("{} {} créé", k.name, id(v).unwrap_or(0))
        )
    );
    println!("{}", id(v).unwrap_or(0));
}

fn updated(k: &K, i: u64) {
    eprintln!("✓ {}", tr!(format!("Updated {} {i}", k.name), format!("{} {i} modifié", k.name)));
}

fn nonempty(m: &Map<String, Value>) -> Result<()> {
    if m.is_empty() {
        return Err(Usage(
            tr!(
                "Nothing to change: give at least one option (see --help)",
                "Rien à changer : donnez au moins une option (voir --help)"
            )
            .into(),
        )
        .into());
    }
    Ok(())
}

fn project_of(v: &Value) -> Option<u64> {
    crate::output::field(v, "project_id").and_then(Value::as_u64)
}

pub fn project(c: &Client, cmd: ProjectCmd) -> Result<()> {
    match cmd {
        ProjectCmd::List(l) => list_generic(c, &PROJECT, &l, &|_| true),
        ProjectCmd::View(v) => view_generic(c, &PROJECT, &v),
        ProjectCmd::Create { title, description, start } => {
            let mut item = json!({"title": title});
            if let Some(d) = description {
                item["description"] = json!(html(&d));
            }
            if let Some(st) = start {
                item["start_date"] = json!(date_or_err(&st)?);
            }
            created(&PROJECT, &c.post("projects", item)?);
            Ok(())
        }
        ProjectCmd::Edit {
            id,
            title,
            description,
            start,
        } => {
            let mut m = Map::new();
            title.map(|t| m.insert("title".into(), json!(t)));
            description.map(|d| m.insert("description".into(), json!(html(&d))));
            if let Some(st) = start {
                m.insert("start_date".into(), json!(date_or_err(&st)?));
            }
            nonempty(&m)?;
            c.put(&format!("projects/{id}"), Value::Object(m.clone()))?;
            // PUT /projects only takes `title` (cle.inserm.fr): say so rather than claim success
            let now = c.get(&format!("projects/{id}"))?;
            if m.get("description")
                .is_some_and(|d| !lgu::same_html(s(&now, "description"), d.as_str().unwrap_or("")))
            {
                bail!(tr!(
                    format!("Labguru did not change the description: its API cannot. Edit it in the web page: lg browse project {id}"),
                    format!(
                        "Labguru n'a pas modifié la description : son API ne le permet pas. Changez-la dans la page web : lg browse project {id}"
                    )
                ));
            }
            updated(&PROJECT, id);
            Ok(())
        }
        // v1 has no DELETE /projects; v2 does
        ProjectCmd::Delete(d) => delete_generic(c, &PROJECT, &d, &format!("v2/projects/{}", d.id)),
    }
}

fn status_name(s: FolderStatus) -> &'static str {
    match s {
        FolderStatus::Completed => "completed",
        FolderStatus::InProgress => "in-progress",
        FolderStatus::Planned => "planned",
    }
}

/// Writes a Description section (folders created by the API have none; their description field is not stored).
pub fn set_description(c: &Client, path: &str, oid: u64, mdtext: &str) -> Result<()> {
    let obj = c.get(&format!("{path}/{oid}"))?;
    let h = html(mdtext);
    let sec = match lgu::sections(&obj).into_iter().find(|x| s(x, "name") == "Description") {
        Some(sec) => c.get(&format!("sections/{}", id(&sec).unwrap_or(0))).unwrap_or(sec),
        None => lgu::add_section(c, &obj, "Description")?,
    };
    if let lgu::TextOutcome::Commented(e) = lgu::put_text(c, &sec, &h)? {
        eprintln!(
            "! {}",
            tr!(
                format!("block {e} has inline comments in Labguru: not overwritten"),
                format!("bloc {e} commenté dans Labguru : non réécrit")
            )
        );
    }
    Ok(())
}

pub fn folder(c: &Client, cmd: FolderCmd) -> Result<()> {
    match cmd {
        FolderCmd::List { project, list } => list_generic(c, &FOLDER, &list, &|r| project.is_none_or(|p| project_of(r) == Some(p))),
        FolderCmd::View(v) => view_generic(c, &FOLDER, &v),
        FolderCmd::Create {
            project,
            title,
            parent,
            status,
            start,
            description,
        } => {
            let mut item = json!({"title": title, "project_id": project});
            if let Some(p) = parent {
                let par = c.get(&format!("milestones/{p}"))?;
                let anc = s(&par, "ancestry");
                item["ancestry"] = json!(if anc.is_empty() { p.to_string() } else { format!("{anc}/{p}") });
            } else {
                item["ancestry"] = Value::Null; // null = top level
            }
            if let Some(st) = status {
                merge(&mut item, lgu::folder_status(status_name(st), None));
            }
            if let Some(st) = start {
                item["start_date"] = json!(date_or_err(&st)?); // only accepted at creation
            }
            let f = c.post("milestones", item.clone())?;
            // Labguru keeps only the first level of `ancestry` at creation; a PUT stores it whole
            if !item["ancestry"].is_null() && f["ancestry"] != item["ancestry"] {
                c.put(
                    &format!("milestones/{}", id(&f).unwrap_or(0)),
                    json!({"ancestry": item["ancestry"]}),
                )?;
            }
            if let Some(d) = description {
                set_description(c, "milestones", id(&f).unwrap_or(0), &d)?;
            }
            created(&FOLDER, &f);
            Ok(())
        }
        FolderCmd::Edit {
            id,
            title,
            status,
            description,
        } => {
            let mut item = Value::Object(Map::new());
            if let Some(t) = title {
                item["title"] = json!(t);
            }
            if let Some(st) = status {
                merge(&mut item, lgu::folder_status(status_name(st), None));
            }
            if item.as_object().is_some_and(|m| !m.is_empty()) {
                c.put(&format!("milestones/{id}"), item)?;
            } else if description.is_none() {
                nonempty(&Map::new())?;
            }
            if let Some(d) = description {
                set_description(c, "milestones", id, &d)?;
            }
            updated(&FOLDER, id);
            Ok(())
        }
        FolderCmd::Delete(d) => delete_generic(c, &FOLDER, &d, &format!("milestones/{}", d.id)),
    }
}

fn merge(a: &mut Value, b: Value) {
    if let (Some(a), Value::Object(b)) = (a.as_object_mut(), b) {
        a.extend(b);
    }
}

fn dates_item(d: &Dates) -> Result<Map<String, Value>> {
    let tz = jiff::tz::TimeZone::system();
    let mut m = Map::new();
    let (mut sd, mut ed) = (None, None);
    if let Some(st) = &d.start {
        sd = Some(date_or_err(st)?.parse::<jiff::civil::Date>()?);
        m.insert("start_date".into(), json!(lgu::iso_utc(st, &tz)?));
    }
    if let Some(en) = &d.end {
        ed = Some(date_or_err(en)?.parse::<jiff::civil::Date>()?);
        m.insert("end_date".into(), json!(lgu::iso_utc(en, &tz)?));
    }
    // the Gantt chart reads duration, not end_date
    match (d.duration, sd, ed) {
        (Some(n), _, _) => {
            m.insert("duration".into(), json!(n));
        }
        (None, Some(a), Some(b)) => {
            m.insert("duration".into(), json!(lgu::days(a, b)));
        }
        _ => {}
    }
    Ok(m)
}

pub fn experiment(c: &Client, cmd: ExperimentCmd) -> Result<()> {
    match cmd {
        ExperimentCmd::List { project, folder, list } => list_generic(c, &EXPERIMENT, &list, &|r| {
            project.is_none_or(|p| project_of(r) == Some(p))
                && folder.is_none_or(|f| crate::output::field(r, "milestone_id").and_then(Value::as_u64) == Some(f))
        }),
        ExperimentCmd::View(v) => view_generic(c, &EXPERIMENT, &v),
        ExperimentCmd::Create {
            project,
            folder,
            title,
            dates,
        } => {
            let mut item = dates_item(&dates)?;
            item.insert("title".into(), json!(title));
            item.insert("project_id".into(), json!(project));
            item.insert("milestone_id".into(), json!(folder)); // required by Labguru
            created(&EXPERIMENT, &c.post("experiments", Value::Object(item))?);
            Ok(())
        }
        ExperimentCmd::Edit { id, title, dates } => {
            let mut item = dates_item(&dates)?;
            if let Some(t) = title {
                item.insert("title".into(), json!(t));
            }
            nonempty(&item)?;
            c.put(&format!("experiments/{id}"), Value::Object(item))?;
            updated(&EXPERIMENT, id);
            Ok(())
        }
        ExperimentCmd::Move { id, folder } => {
            c.put(&format!("experiments/{id}"), json!({"milestone_id": folder}))?;
            eprintln!(
                "✓ {}",
                tr!(
                    format!("Moved experiment {id} to folder {folder}"),
                    format!("Expérience {id} déplacée dans le dossier {folder}")
                )
            );
            Ok(())
        }
        ExperimentCmd::Delete(d) => delete_generic(c, &EXPERIMENT, &d, &format!("experiments/{}", d.id)),
        ExperimentCmd::Flag { .. } | ExperimentCmd::Sign { .. } => explain_limits(Some(c), cmd),
    }
}

/// Flags and signatures: say clearly what the API cannot do (works without being logged in).
pub fn explain_limits(c: Option<&Client>, cmd: ExperimentCmd) -> Result<()> {
    match cmd {
        ExperimentCmd::Flag { id } => {
            let documented = c.is_some_and(|c| c.caps.as_ref().is_some_and(|x| !x.endpoints.is_empty()) && c.supports("POST", &format!("experiments/{id}/set_flag_as")));
            let extra = if documented {
                tr!(" This instance documents experiments/{id}/set_flag_as, but lg does not set flags for you.", " Cette instance documente experiments/{id}/set_flag_as, mais lg ne pose pas de flag à votre place.").replace("{id}", &id.to_string())
            } else {
                String::new()
            };
            bail!(tr!(
                "The Successful / Failed flag cannot be set through the Labguru API by lg.{extra} Set it in the web page: lg browse experiment {id}",
                "Le flag Successful / Failed ne peut pas être posé par lg via l'API Labguru.{extra} Posez-le dans la page web : lg browse experiment {id}"
            ).replace("{extra}", &extra).replace("{id}", &id.to_string()))
        }
        ExperimentCmd::Sign { id } => bail!(tr!(
            "Experiments cannot be signed through the Labguru API, and lg never signs. Sign in the web page: lg browse experiment {id}",
            "On ne peut pas signer une expérience via l'API Labguru, et lg ne signe jamais. Signez dans la page web : lg browse experiment {id}"
        ).replace("{id}", &id.to_string())),
        _ => Ok(()),
    }
}

/// (API path, id) of the container named by --project/--folder/--experiment/--note/--paper.
fn target(t: &Target) -> (&'static str, u64) {
    match (t.project, t.folder, t.experiment, t.note, t.paper) {
        (Some(i), ..) => ("projects", i),
        (_, Some(i), ..) => ("milestones", i),
        (_, _, Some(i), ..) => ("experiments", i),
        (_, _, _, Some(i), _) => ("notes", i),
        (.., Some(i)) => ("papers", i),
        _ => unreachable!("clap requires one target"),
    }
}

fn section_target(t: &Target) -> Result<(&'static str, u64)> {
    let (p, i) = target(t);
    if p == "notes" || p == "papers" {
        return Err(Usage(
            tr!(
                "Sections belong to a --project, --folder or --experiment",
                "Les sections appartiennent à un --project, --folder ou --experiment"
            )
            .into(),
        )
        .into());
    }
    Ok((p, i))
}

const SECTION_COLS: &[(&str, &str)] = &[("ID", "id"), ("NAME", "name"), ("POSITION", "position")];

pub fn section(c: &Client, cmd: SectionCmd) -> Result<()> {
    match cmd {
        SectionCmd::List { target: t, list } => {
            let (p, i) = section_target(&t)?;
            let obj = c.get(&format!("{p}/{i}"))?;
            if list.web {
                return open_obj(c, &obj);
            }
            let mut rows = lgu::sections(&obj);
            rows.truncate(list.limit);
            print_list(&rows, SECTION_COLS, &list.out)
        }
        SectionCmd::View(v) => {
            let sec = c.get(&format!("sections/{}", v.id))?;
            if v.web {
                return Err(Usage(
                    tr!(
                        "a section has no web page of its own: open its experiment with lg browse experiment <id>",
                        "une section n'a pas de page web à elle : ouvrez son expérience avec lg browse experiment <id>"
                    )
                    .into(),
                )
                .into());
            }
            print_object(
                &sec,
                &[
                    ("Name", "name"),
                    ("ID", "id"),
                    ("Container", "container_type"),
                    ("Container id", "container_id"),
                ],
                &v.out,
            )?;
            if !v.out.is_json() {
                for e in sec.get("elements").and_then(Value::as_array).into_iter().flatten() {
                    println!(
                        "\n[{} {}]\n{}",
                        s(e, "element_type"),
                        id(e).unwrap_or(0),
                        md::html_to_text(s(e, "data"))
                    );
                }
            }
            Ok(())
        }
        SectionCmd::Create { target: t, name, body } => {
            let (p, i) = section_target(&t)?;
            let container = c.get(&format!("{p}/{i}"))?;
            let sec = lgu::add_section(c, &container, &name)?;
            if let Some(b) = read_body(&body)? {
                lgu::put_text(c, &sec, &html(&b))?;
            }
            eprintln!(
                "✓ {}",
                tr!(
                    format!("Created section {}", id(&sec).unwrap_or(0)),
                    format!("Section {} créée", id(&sec).unwrap_or(0))
                )
            );
            println!("{}", id(&sec).unwrap_or(0));
            Ok(())
        }
        SectionCmd::Edit { id: sid, name, body } => {
            let b = read_body(&body)?;
            if name.is_none() && b.is_none() {
                nonempty(&Map::new())?;
            }
            let renamed = name.is_some();
            if let Some(n) = name {
                c.put(&format!("sections/{sid}"), json!({"name": n}))?;
            }
            if let Some(b) = b {
                let sec = c.get(&format!("sections/{sid}"))?;
                let outcome = lgu::put_text(c, &sec, &html(&b))?;
                if outcome == lgu::TextOutcome::Unchanged && !renamed {
                    eprintln!(
                        "✓ {}",
                        tr!(format!("Section {sid} already up to date"), format!("Section {sid} déjà à jour"))
                    );
                    return Ok(());
                }
                if let lgu::TextOutcome::Commented(e) = outcome {
                    bail!(tr!(
                        format!(
                            "Text block {e} has inline comments (lg-comment) in Labguru; lg never overwrites it. Edit it in the web page."
                        ),
                        format!(
                            "Le bloc {e} contient des commentaires en ligne (lg-comment) dans Labguru ; lg ne l'écrase jamais. Modifiez-le dans la page web."
                        )
                    ));
                }
            }
            eprintln!("✓ {}", tr!(format!("Updated section {sid}"), format!("Section {sid} modifiée")));
            Ok(())
        }
        SectionCmd::Delete(d) => {
            let k = K {
                name: "section",
                path: "sections",
                web: "",
                cols: &[],
                view: &[],
            };
            delete_generic(c, &k, &d, &format!("sections/{}", d.id))
        }
    }
}

pub fn note(c: &Client, cmd: NoteCmd) -> Result<()> {
    match cmd {
        NoteCmd::List { project, list } => list_generic(c, &NOTE, &list, &|r| project.is_none_or(|p| project_of(r) == Some(p))),
        NoteCmd::View(v) => view_generic(c, &NOTE, &v),
        NoteCmd::Create { project, title, body } => {
            let b = read_body(&body)?.unwrap_or_default();
            created(
                &NOTE,
                &c.post("notes", json!({"title": title, "project_id": project, "body": html(&b)}))?,
            );
            Ok(())
        }
        NoteCmd::Edit { id, title, body } => {
            let mut m = Map::new();
            title.map(|t| m.insert("title".into(), json!(t)));
            if let Some(b) = read_body(&body)? {
                m.insert("body".into(), json!(html(&b)));
            }
            nonempty(&m)?;
            c.put(&format!("notes/{id}"), Value::Object(m))?;
            updated(&NOTE, id);
            Ok(())
        }
        NoteCmd::Delete(d) => delete_generic(c, &NOTE, &d, &format!("notes/{}", d.id)),
    }
}

fn paper_fields(m: &mut Map<String, Value>, f: PaperFields) -> Result<()> {
    f.authors.map(|v| m.insert("authors".into(), json!(v)));
    f.journal.map(|v| m.insert("journal".into(), json!(v)));
    if let Some(d) = f.date {
        m.insert("publication_date".into(), json!(date_or_err(&d)?));
    }
    f.doi.map(|v| m.insert("doi".into(), json!(v)));
    f.url.map(|v| m.insert("url".into(), json!(v)));
    Ok(())
}

/// Creates the missing papers of a BibTeX text (same DOI or title = already there); links them to a project.
pub fn add_bib(c: &Client, text: &str, project: Option<&Value>, apply: bool, log: &mut dyn FnMut(String)) -> Result<usize> {
    let mut papers = c.all("papers", &[])?;
    let mut changes = 0;
    for b in bib::parse(text) {
        let item = bib::paper_item(&b);
        let Some(name) = item.get("name").and_then(Value::as_str).map(String::from) else {
            log(tr!(
                format!("! entry {} has no title: skipped", b["key"]),
                format!("! entrée {} sans titre : ignorée", b["key"])
            ));
            continue;
        };
        let short: String = name.chars().take(70).collect();
        let paper = match lgu::find_paper(&papers, &item).cloned() {
            Some(p) => p,
            None => {
                changes += 1;
                log(format!("+ {} « {short} »", tr!("paper", "article")));
                if !apply {
                    continue;
                }
                let p = c.post("papers", Value::Object(item))?;
                papers.push(p.clone());
                p
            }
        };
        if let Some(pr) = project {
            let linked = pr.get("links").and_then(Value::as_array).is_some_and(|l| {
                l.iter()
                    .any(|x| x.as_str() == Some(s(&paper, "uuid")) || s(x, "target_uuid") == s(&paper, "uuid"))
            });
            if !linked {
                changes += 1;
                log(format!("+ {} « {short} »", tr!("link to project:", "lien au projet :")));
                if apply {
                    lgu::link_paper(c, pr, &paper)?;
                }
            }
        }
    }
    Ok(changes)
}

pub fn paper(c: &Client, cmd: PaperCmd) -> Result<()> {
    match cmd {
        PaperCmd::List(l) => list_generic(c, &PAPER, &l, &|_| true),
        PaperCmd::View(v) => view_generic(c, &PAPER, &v),
        PaperCmd::Create { title, fields } => {
            let mut m = Map::new();
            m.insert("name".into(), json!(title));
            paper_fields(&mut m, fields)?;
            created(&PAPER, &c.post("papers", Value::Object(m))?);
            Ok(())
        }
        PaperCmd::Add { bib, project } => {
            let text = std::fs::read_to_string(&bib).with_context(|| bib.display().to_string())?;
            let pr = project.map(|p| c.get(&format!("projects/{p}"))).transpose()?;
            let n = add_bib(c, &text, pr.as_ref(), true, &mut |l| eprintln!("{l}"))?;
            eprintln!("✓ {}", tr!(format!("{n} changes"), format!("{n} changements")));
            Ok(())
        }
        PaperCmd::Edit { id, title, fields } => {
            let mut m = Map::new();
            title.map(|t| m.insert("name".into(), json!(t)));
            paper_fields(&mut m, fields)?;
            nonempty(&m)?;
            c.put(&format!("papers/{id}"), Value::Object(m))?;
            updated(&PAPER, id);
            Ok(())
        }
        PaperCmd::Delete(d) => delete_generic(c, &PAPER, &d, &format!("papers/{}", d.id)),
    }
}

const ATTACH_COLS: &[(&str, &str)] = &[
    ("ID", "id"),
    ("TITLE", "title"),
    ("FILE", "attachment_file_name"),
    ("SIZE", "attachment_file_size"),
    ("TYPE", "attachment_content_type"),
];

pub fn attachment(c: &Client, cmd: AttachmentCmd) -> Result<()> {
    match cmd {
        AttachmentCmd::List { target: t, list } => {
            let (p, i) = target(&t);
            let obj = c.get(&format!("{p}/{i}"))?;
            if list.web {
                return open_obj(c, &obj);
            }
            let mut rows: Vec<Value> = obj.get("attachments").and_then(Value::as_array).cloned().unwrap_or_default();
            rows.truncate(list.limit);
            print_list(&rows, ATTACH_COLS, &list.out)
        }
        AttachmentCmd::View(v) => {
            let a = c.get(&format!("attachments/{}", v.id))?;
            if v.web {
                return open_obj(c, &a);
            }
            print_object(
                &a,
                &[
                    ("Title", "title"),
                    ("ID", "id"),
                    ("File", "attachment_file_name"),
                    ("Size", "attachment_file_size"),
                    ("Type", "attachment_content_type"),
                    ("Attached to", "attach_to_uuid"),
                ],
                &v.out,
            )
        }
        AttachmentCmd::Upload { file, target: t, title } => {
            if !file.is_file() {
                return Err(Usage(tr!(
                    format!("{}: no such file", file.display()),
                    format!("{} : fichier introuvable", file.display())
                ))
                .into());
            }
            let (p, i) = target(&t);
            let obj = c.get(&format!("{p}/{i}"))?;
            let title = title.unwrap_or_else(|| file.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default());
            let a = c.upload(&file, Some(s(&obj, "uuid")), &title)?;
            eprintln!(
                "✓ {}",
                tr!(
                    format!("Uploaded {} (attachment {})", file.display(), id(&a).unwrap_or(0)),
                    format!("{} envoyé (pièce jointe {})", file.display(), id(&a).unwrap_or(0))
                )
            );
            println!("{}", id(&a).unwrap_or(0));
            Ok(())
        }
        AttachmentCmd::Download { id: aid, output } => {
            let a = c.get(&format!("attachments/{aid}"))?;
            // `url` is the web page, not the file
            let url = Some(s(&a, "download_url"))
                .filter(|u| !u.is_empty())
                .map_or_else(|| lgu::original_url(&a), String::from);
            let name = Some(s(&a, "attachment_file_name"))
                .filter(|n| !n.is_empty())
                .unwrap_or(s(&a, "title"))
                .to_string();
            let name = std::path::Path::new(&name)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| format!("attachment-{aid}"));
            let out = output.unwrap_or_else(|| name.into());
            if out.as_os_str() == "-" {
                c.download(&url, &mut std::io::stdout().lock())?;
            } else {
                let tmp = out.with_extension("lg-part");
                let mut f = std::fs::File::create(&tmp).with_context(|| tmp.display().to_string())?;
                let n = c.download(&url, &mut f);
                if n.is_err() {
                    let _ = std::fs::remove_file(&tmp);
                }
                let n = n?;
                std::fs::rename(&tmp, &out)?;
                eprintln!("✓ {} ({n} B)", out.display());
            }
            Ok(())
        }
        AttachmentCmd::Delete(d) => {
            let k = K {
                name: "attachment",
                path: "attachments",
                web: "",
                cols: &[],
                view: &[],
            };
            delete_generic(c, &k, &d, &format!("attachments/{}", d.id))
        }
    }
}

pub fn tag(c: &Client, cmd: TagCmd) -> Result<()> {
    let (t, name) = match &cmd {
        TagCmd::List { target, .. } => (target, None),
        TagCmd::Create { target, name } | TagCmd::Delete { target, name } => (target, Some(name.clone())),
    };
    let (p, i) = target(t);
    let obj = c.get(&format!("{p}/{i}"))?;
    let item = |n: &str| json!({"tag": n, "class_name": s(&obj, "class_name"), "item_id": i});
    match cmd {
        TagCmd::List { out, .. } => {
            let rows: Vec<Value> = lgu::tag_names(&obj).into_iter().map(|n| json!({"name": n})).collect();
            print_list(&rows, &[("NAME", "name")], &out)
        }
        TagCmd::Create { .. } => {
            let n = name.unwrap_or_default();
            c.post("tags", item(&n))?;
            eprintln!("✓ {}", tr!(format!("Tagged {p}/{i} with {n}"), format!("Tag {n} ajouté à {p}/{i}")));
            Ok(())
        }
        TagCmd::Delete { .. } => {
            let n = name.unwrap_or_default();
            // DELETE /tags/{id of the tagged object}, with the tag in `item`
            c.require("DELETE", &format!("tags/{i}"), "lg tag delete")?;
            c.send("DELETE", &format!("tags/{i}"), Some(item(&n)), Map::new())?;
            eprintln!(
                "✓ {}",
                tr!(format!("Removed tag {n} from {p}/{i}"), format!("Tag {n} retiré de {p}/{i}"))
            );
            Ok(())
        }
    }
}

pub fn browse(c: &Client, k: Kind, i: u64) -> Result<()> {
    open_url(&object_url(c, kind(k), i)?)
}

/// "item[title]=x" → {"item": {"title": "x"}}.
pub fn parse_fields(fields: &[String]) -> Result<Map<String, Value>> {
    let mut root = Map::new();
    for f in fields {
        let Some((k, v)) = f.split_once('=') else {
            return Err(Usage(tr!(
                format!("bad field '{f}': use key=value"),
                format!("champ « {f} » invalide : utilisez clé=valeur")
            ))
            .into());
        };
        let mut keys: Vec<&str> = vec![k.split('[').next().unwrap_or(k)];
        keys.extend(k.match_indices('[').filter_map(|(i, _)| k[i + 1..].split(']').next()));
        let mut cur = &mut root;
        for key in &keys[..keys.len() - 1] {
            cur = cur
                .entry(key.to_string())
                .or_insert_with(|| json!({}))
                .as_object_mut()
                .ok_or_else(|| Usage(f.to_string()))?;
        }
        cur.insert(keys[keys.len() - 1].to_string(), json!(v));
    }
    Ok(root)
}

pub fn api(c: &Client, a: ApiArgs) -> Result<()> {
    let method = a
        .method
        .as_deref()
        .unwrap_or(if a.input.is_some() { "POST" } else { "GET" })
        .to_ascii_uppercase();
    let mut body = match &a.input {
        Some(p) => {
            let mut sb = String::new();
            if p.as_os_str() == "-" {
                std::io::stdin().read_to_string(&mut sb)?;
            } else {
                sb = std::fs::read_to_string(p).with_context(|| p.display().to_string())?;
            }
            match serde_json::from_str::<Value>(&sb).context("--input")? {
                Value::Object(m) => m,
                _ => return Err(Usage("--input: JSON object expected".into()).into()),
            }
        }
        None => Map::new(),
    };
    let v = if method == "GET" {
        let q: Vec<(String, String)> = a
            .fields
            .iter()
            .filter_map(|f| f.split_once('='))
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        if a.fields.iter().any(|f| !f.contains('=')) {
            parse_fields(&a.fields)?;
        }
        if a.paginate {
            Value::Array(c.all(&a.path, &q)?)
        } else {
            c.get_q(&a.path, &q)?
        }
    } else {
        body.extend(parse_fields(&a.fields)?);
        c.request(&method, &a.path, &[], Some(Value::Object(body)))?
    };
    let out = OutArgs {
        json: a.out.json.clone(),
        jq: a.out.jq.clone(),
        template: a.out.template.clone(),
    };
    let v = match (&out.json, &v) {
        (Some(f), Value::Array(rows)) => Value::Array(crate::output::select(rows, f)?),
        (Some(f), x) => crate::output::select(std::slice::from_ref(x), f)?.remove(0),
        _ => v,
    };
    crate::output::emit_json(&mut std::io::stdout().lock(), &v, &out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fields() {
        let m = parse_fields(&["item[title]=A=B".into(), "item[x][y]=1".into(), "page=2".into()]).unwrap();
        assert_eq!(Value::Object(m), json!({"item": {"title": "A=B", "x": {"y": "1"}}, "page": "2"}));
        assert!(parse_fields(&["nokey".into()]).is_err());
    }
}
