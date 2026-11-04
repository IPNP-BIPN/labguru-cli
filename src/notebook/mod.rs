//! lg notebook: a directory of Markdown files published to one Labguru project.

pub mod entry;

use crate::api::Client;
use crate::labguru::{self as lgu, TextOutcome, id, s};
use crate::md;
use anyhow::{Context, Result, bail};
use entry::{Entry, Status};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NbConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    pub project: String,
    #[serde(default)]
    pub data_dirs: Vec<String>,
    #[serde(default)]
    pub layout: Layout,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Layout {
    /// "by-status": "<folder> — completed / in progress / planned" top-level folders; "single": one folder.
    #[serde(default = "by_status")]
    pub style: String,
    pub completed: String,
    pub in_progress: String,
    pub planned: String,
    pub completed_text: String,
    pub in_progress_text: String,
    pub planned_text: String,
}

fn by_status() -> String {
    "by-status".into()
}

impl Default for Layout {
    fn default() -> Self {
        Layout {
            style: by_status(),
            completed: "Completed".into(),
            in_progress: "In Progress".into(),
            planned: "Planned".into(),
            completed_text: "Finished experiments: the work is done.".into(),
            in_progress_text: "Started experiments: their entry already records results.".into(),
            planned_text: "Planned experiments: nothing done yet, the entry describes what is intended.".into(),
        }
    }
}

impl Layout {
    pub fn french() -> Layout {
        Layout {
            style: by_status(),
            completed: "terminées".into(),
            in_progress: "en cours".into(),
            planned: "prévues".into(),
            completed_text: "Expériences terminées : le travail est fini.".into(),
            in_progress_text: "Expériences commencées : leur entrée consigne déjà des résultats.".into(),
            planned_text: "Expériences prévues : rien n'est encore réalisé, l'entrée décrit ce qui est envisagé.".into(),
        }
    }

    fn label(&self, st: Status) -> (&str, &str) {
        match st {
            Status::Done => (&self.completed, &self.completed_text),
            Status::InProgress => (&self.in_progress, &self.in_progress_text),
            Status::Planned => (&self.planned, &self.planned_text),
        }
    }
}

pub const CONFIG: &str = "lg.toml";
const SKIP_DIRS: &[&str] = &["notes", "pieces", "attachments", "figures", "images", "data"];

pub fn find_root(root: Option<&Path>) -> Result<PathBuf> {
    if let Some(r) = root {
        if r.join(CONFIG).is_file() {
            return Ok(r.to_path_buf());
        }
        bail!(tr!(
            format!("{} has no {CONFIG}; create one with: lg notebook init <dir>", r.display()),
            format!("{} n'a pas de {CONFIG} ; créez-le avec : lg notebook init <dossier>", r.display())
        ));
    }
    let cwd = std::env::current_dir()?;
    cwd.ancestors()
        .find(|d| d.join(CONFIG).is_file())
        .map(Path::to_path_buf)
        .ok_or_else(|| {
            anyhow::anyhow!(tr!(
                format!("No {CONFIG} here or in a parent directory. Create a notebook with: lg notebook init <dir>"),
                format!("Aucun {CONFIG} ici ni dans un dossier parent. Créez un cahier avec : lg notebook init <dossier>")
            ))
        })
}

pub fn load_config(root: &Path) -> Result<NbConfig> {
    let p = root.join(CONFIG);
    let t = std::fs::read_to_string(&p).with_context(|| p.display().to_string())?;
    toml::from_str(&t).with_context(|| tr!(format!("{} is not valid", p.display()), format!("{} n'est pas valide", p.display())))
}

/// One notebook folder (sub-directory) and its entries.
pub struct Folder {
    pub dir: String,
    pub title: String,
    pub readme: String,
    pub status: Option<Status>,
    pub entries: Vec<(String, Entry)>,
}

fn read(p: &Path) -> Result<String> {
    std::fs::read_to_string(p).with_context(|| p.display().to_string())
}

pub fn scan_folders(root: &Path) -> Result<Vec<Folder>> {
    let mut out = Vec::new();
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(root)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort();
    for d in dirs {
        let name = d.file_name().unwrap().to_string_lossy().into_owned();
        if name.starts_with('.') || name.starts_with('_') || SKIP_DIRS.contains(&name.as_str()) {
            continue;
        }
        let mut files: Vec<PathBuf> = std::fs::read_dir(&d)?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "md"))
            .collect();
        files.sort();
        let readme_path = d.join("README.md");
        if files.is_empty() && !readme_path.is_file() {
            continue;
        }
        let (status, readme) = if readme_path.is_file() {
            let t = read(&readme_path)?;
            let (kv, body) = entry::front_matter(&t);
            let st = kv
                .iter()
                .find(|(k, _)| k == "statut" || k == "status")
                .and_then(|(_, v)| Status::parse(v));
            (st, body.to_string())
        } else {
            (None, String::new())
        };
        let title = entry::title_of(&readme).unwrap_or_else(|| name.clone());
        let mut entries = Vec::new();
        for f in files {
            let fname = f.file_name().unwrap().to_string_lossy().into_owned();
            if fname.eq_ignore_ascii_case("README.md") {
                continue;
            }
            let stem = f.file_stem().unwrap().to_string_lossy().into_owned();
            let e = entry::parse_entry(&read(&f)?, &stem).with_context(|| f.display().to_string())?;
            entries.push((fname, e));
        }
        out.push(Folder {
            dir: name,
            title,
            readme: entry::without_title(&readme),
            status,
            entries,
        });
    }
    Ok(out)
}

/// Status of the main folder: header, else all done → completed, any started → in progress, else planned.
pub fn folder_status(f: &Folder) -> Status {
    if let Some(s) = f.status {
        return s;
    }
    let g: Vec<Status> = f.entries.iter().map(|(_, e)| entry::group(e)).collect();
    if !g.is_empty() && g.iter().all(|s| *s == Status::Done) {
        Status::Done
    } else if g.iter().any(|s| *s != Status::Planned) {
        Status::InProgress
    } else {
        Status::Planned
    }
}

pub struct Report {
    pub changes: usize,
    pub warnings: usize,
}

struct Ctx<'a> {
    c: &'a Client,
    apply: bool,
    root: PathBuf,
    project: String,
    tz: jiff::tz::TimeZone,
    lists: HashMap<&'static str, Vec<Value>>,
    out: &'a mut dyn FnMut(String),
    changes: usize,
    warnings: usize,
}

impl Ctx<'_> {
    fn change(&mut self, sym: &str, msg: String) {
        self.changes += 1;
        (self.out)(format!("{sym} {msg}"));
    }

    fn warn(&mut self, msg: String) {
        self.warnings += 1;
        (self.out)(format!("! {msg}"));
    }

    fn list(&mut self, path: &'static str) -> Result<&mut Vec<Value>> {
        if !self.lists.contains_key(path) {
            let v = self.c.all(path, &[])?;
            self.lists.insert(path, v);
        }
        Ok(self.lists.get_mut(path).unwrap())
    }

    /// By external_uuid, else by title, within the project.
    fn find(&mut self, path: &'static str, uuid: &str, title: &str, project: Option<u64>) -> Result<Option<Value>> {
        let rows: Vec<Value> = self
            .list(path)?
            .iter()
            .filter(|x| project.is_none() || crate::output::field(x, "project_id").and_then(Value::as_u64) == project)
            .cloned()
            .collect();
        Ok(rows
            .iter()
            .find(|x| !uuid.is_empty() && s(x, "external_uuid") == uuid)
            .or_else(|| rows.iter().find(|x| s(x, "title") == title))
            .cloned())
    }

    fn create(&mut self, path: &'static str, item: Value) -> Result<Value> {
        let v = self.c.post(path, item)?;
        self.list(path)?.push(v.clone());
        Ok(v)
    }

    fn uuid(&self, rest: &str) -> String {
        format!("lg:{}/{rest}", self.project)
    }

    /// Markdown → HTML; local images attached to `owner` (title = path in the notebook) and shown full size.
    fn render(&mut self, mdtext: &str, base: &Path, owner: Option<&Value>) -> Result<(String, bool)> {
        let mut urls: HashMap<String, String> = HashMap::new();
        let mut pending = false;
        let have: Vec<Value> = owner
            .and_then(|o| o.get("attachments"))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        for src in md::local_images(mdtext) {
            let decoded = src.replace("%20", " ");
            let file = base.join(&decoded);
            let rel = file.strip_prefix(&self.root).unwrap_or(&file).to_string_lossy().replace('\\', "/");
            if !file.is_file() {
                self.warn(tr!(
                    format!("image {rel}: file not found, left as is"),
                    format!("image {rel} : fichier introuvable, laissée telle quelle")
                ));
                continue;
            }
            if let Some(a) = have.iter().find(|a| s(a, "title") == rel) {
                urls.insert(src, lgu::original_url(a));
                continue;
            }
            self.change("+", tr!(format!("attach image {rel}"), format!("joindre l'image {rel}")));
            pending = true;
            if let (true, Some(o)) = (self.apply, owner) {
                let a = self.c.upload(&file, Some(s(o, "uuid")), &rel)?;
                urls.insert(src, lgu::original_url(&a));
            }
        }
        Ok((md::to_html(mdtext, &mut |src| urls.get(src).cloned()), pending))
    }

    /// Makes the named sections of `obj` hold these texts (Markdown).
    fn fill(&mut self, path: &str, obj: Option<&Value>, texts: &[(String, String)], base: &Path, label: &str) -> Result<()> {
        let full = match obj.and_then(id) {
            Some(i) => Some(self.c.get(&format!("{path}/{i}"))?),
            None => None,
        };
        for (name, mdtext) in texts {
            let (h, _) = self.render(mdtext, base, full.as_ref())?;
            let Some(full) = &full else {
                self.change("+", tr!(format!("{label}: section {name}"), format!("{label} : section {name}")));
                continue;
            };
            let sec = lgu::sections(full).into_iter().find(|x| s(x, "name") == name);
            let sec = match sec {
                Some(x) if x.get("elements").is_some() => Some(x),
                Some(x) => Some(self.c.get(&format!("sections/{}", id(&x).unwrap_or(0)))?),
                None => None,
            };
            match sec {
                None => {
                    self.change("+", tr!(format!("{label}: section {name}"), format!("{label} : section {name}")));
                    if self.apply {
                        let sec = lgu::add_section(self.c, full, name)?;
                        lgu::put_text(self.c, &sec, &h)?;
                    }
                }
                Some(sec) => match lgu::text_plan(self.c, &sec, &h)? {
                    TextOutcome::Unchanged => {}
                    TextOutcome::Commented(e) => self.warn(tr!(
                        format!("{label}: section {name} has inline comments in Labguru (block {e}): not overwritten"),
                        format!("{label} : section {name} commentée dans Labguru (bloc {e}) : non réécrite")
                    )),
                    _ => {
                        self.change("~", tr!(format!("{label}: section {name}"), format!("{label} : section {name}")));
                        if self.apply {
                            lgu::put_text(self.c, &sec, &h)?;
                        }
                    }
                },
            }
        }
        Ok(())
    }

    /// Creates or updates a folder; returns it (None in a dry run when it does not exist yet).
    fn folder(
        &mut self,
        pid: Option<u64>,
        uuid: &str,
        title: &str,
        status: Status,
        completion: Option<String>,
        start: Option<String>,
    ) -> Result<Option<Value>> {
        let fields = lgu::folder_status(status.key(), completion.as_deref());
        let existing = match pid {
            Some(p) => self.find("milestones", uuid, title, Some(p))?,
            None => None,
        };
        let Some(f) = existing else {
            self.change(
                "+",
                tr!(
                    format!("folder « {title} » ({})", lgu::status_label(status.key())),
                    format!("dossier « {title} » ({})", lgu::status_label(status.key()))
                ),
            );
            if !self.apply {
                return Ok(None);
            }
            let mut item = json!({"title": title, "external_uuid": uuid, "project_id": pid, "ancestry": null});
            if let Some(st) = start {
                item["start_date"] = json!(st); // accepted at creation only
            }
            item.as_object_mut().unwrap().extend(fields.as_object().unwrap().clone());
            return Ok(Some(self.create("milestones", item)?));
        };
        let mut diff = Map::new();
        for k in ["completed", "current"] {
            if f.get(k).and_then(Value::as_bool).unwrap_or(false) != fields[k].as_bool().unwrap_or(false) {
                diff.insert(k.into(), fields[k].clone());
            }
        }
        let want_date = fields["compleation_date"].as_str().unwrap_or("");
        if s(&f, "compleation_date").get(..10).unwrap_or("") != want_date.get(..10).unwrap_or("")
            && !(want_date.is_empty() && f["compleation_date"].is_null())
        {
            diff.insert("compleation_date".into(), fields["compleation_date"].clone());
        }
        if s(&f, "title") != title {
            diff.insert("title".into(), json!(title));
        }
        if s(&f, "external_uuid") != uuid {
            diff.insert("external_uuid".into(), json!(uuid));
        }
        if f.get("ancestry").is_some_and(|a| !a.is_null() && a != "") {
            diff.insert("ancestry".into(), Value::Null); // top level: Progress only sorts top-level folders
        }
        if !diff.is_empty() {
            let keys: Vec<&String> = diff.keys().collect();
            self.change(
                "~",
                tr!(format!("folder « {title} »: {keys:?}"), format!("dossier « {title} » : {keys:?}")),
            );
            if self.apply {
                self.c.put(&format!("milestones/{}", id(&f).unwrap_or(0)), Value::Object(diff))?;
            }
        }
        Ok(Some(f))
    }
}

pub struct PushOpts {
    pub apply: bool,
    pub project: Option<String>,
    pub today: jiff::civil::Date,
    pub tz: jiff::tz::TimeZone,
}

/// Dry run (apply = false) or real push. Idempotent; never deletes an experiment.
pub fn push(c: &Client, root: &Path, o: PushOpts, out: &mut dyn FnMut(String)) -> Result<Report> {
    let cfg = load_config(root)?;
    let project = o.project.clone().unwrap_or(cfg.project.clone());
    let folders = scan_folders(root)?;
    let data: Vec<PathBuf> = cfg.data_dirs.iter().map(|d| root.join(d)).collect();
    let times = entry::day_times(entry::scan(&data), &o.tz);
    let mut x = Ctx {
        c,
        apply: o.apply,
        root: root.to_path_buf(),
        project: project.clone(),
        tz: o.tz.clone(),
        lists: HashMap::new(),
        out,
        changes: 0,
        warnings: 0,
    };

    // project
    let ptext = ["projet.md", "project.md"]
        .iter()
        .map(|f| root.join(f))
        .find(|p| p.is_file())
        .map(|p| read(&p))
        .transpose()?;
    let pdesc = ptext.as_deref().map(|t| md::to_html(&entry::without_title(t), &mut |_| None));
    let mut proj = x.find("projects", "", &project, None)?;
    if proj.is_none() {
        x.change("+", tr!(format!("project « {project} »"), format!("projet « {project} »")));
        if o.apply {
            let mut item = json!({"title": project});
            if let Some(d) = &pdesc {
                item["description"] = json!(d);
            }
            proj = Some(x.create("projects", item)?);
        }
    } else if let (Some(p), Some(d)) = (&proj, &pdesc)
        && !lgu::same_html(s(p, "description"), d)
    {
        // PUT /projects only takes `title`: the description is set at creation only
        x.warn(tr!(
            format!("project « {project} »: its description differs from the notebook; the API cannot change it, edit it in Labguru"),
            format!("projet « {project} » : sa description diffère du cahier ; l'API ne peut pas la modifier, changez-la dans Labguru")
        ));
    }
    let pid = proj.as_ref().and_then(id);
    let by_status = cfg.layout.style != "single";

    for f in &folders {
        let label = |st: Status| format!("{} — {}", f.title, cfg.layout.label(st).0);
        let groups: BTreeMap<Status, Vec<&(String, Entry)>> = f.entries.iter().fold(BTreeMap::new(), |mut m, e| {
            m.entry(entry::group(&e.1)).or_insert_with(Vec::new).push(e);
            m
        });
        let spans: HashMap<&str, entry::Span> = f
            .entries
            .iter()
            .filter_map(|(n, e)| entry::span(e, o.today, &times, &x.tz).map(|sp| (n.as_str(), sp)))
            .collect();
        let first = |es: &[&(String, Entry)]| {
            es.iter()
                .filter_map(|(n, _)| spans.get(n.as_str()).map(|s| s.start_day))
                .min()
                .map(|d| d.to_string())
        };
        let last = |es: &[&(String, Entry)]| {
            es.iter()
                .filter_map(|(n, _)| spans.get(n.as_str()).map(|s| s.end_day))
                .max()
                .map(|d| d.to_string())
        };
        let all: Vec<&(String, Entry)> = f.entries.iter().collect();

        // the folder itself, with its README as Description
        let fuuid = x.uuid(&f.dir);
        let fst = folder_status(f);
        let main = x.folder(pid, &fuuid, &f.title, fst, last(&all), first(&all))?;
        if !f.readme.is_empty() {
            let texts = [("Description".to_string(), f.readme.clone())];
            x.fill("milestones", main.as_ref(), &texts, &root.join(&f.dir), &f.title)?;
        }

        // one top-level folder per status (or the folder itself)
        let mut target: HashMap<Status, Option<Value>> = HashMap::new();
        for (st, es) in &groups {
            if !by_status {
                target.insert(*st, main.clone());
                continue;
            }
            let g = x.folder(pid, &format!("{fuuid}#{}", st.key()), &label(*st), *st, last(es), first(es))?;
            let texts = [("Description".to_string(), cfg.layout.label(*st).1.to_string())];
            x.fill("milestones", g.as_ref(), &texts, root, &label(*st))?;
            target.insert(*st, g);
        }

        // experiments
        let mut moved_out: HashMap<u64, usize> = HashMap::new();
        for (st, es) in &groups {
            for (fname, e) in es {
                let uuid = x.uuid(&format!("{}/{fname}", f.dir));
                let dest = target.get(st).cloned().flatten();
                let dest_title = if by_status { label(*st) } else { f.title.clone() };
                let mut item = Map::new();
                item.insert("title".into(), json!(e.title));
                item.insert("external_uuid".into(), json!(uuid));
                if let Some(sp) = spans.get(fname.as_str()) {
                    item.insert("start_date".into(), json!(sp.start));
                    item.insert("end_date".into(), json!(sp.end));
                    item.insert("duration".into(), json!(sp.duration)); // the Gantt chart reads duration
                }
                let existing = match pid {
                    Some(p) => x.find("experiments", &uuid, &e.title, Some(p))?,
                    None => None,
                };
                let exp = match existing {
                    None => {
                        x.change(
                            "+",
                            tr!(
                                format!("experiment « {} » → {dest_title}", e.title),
                                format!("expérience « {} » → {dest_title}", e.title)
                            ),
                        );
                        match (o.apply, &dest) {
                            (true, Some(d)) => {
                                item.insert("project_id".into(), json!(pid));
                                item.insert("milestone_id".into(), json!(id(d)));
                                Some(x.create("experiments", Value::Object(item))?)
                            }
                            _ => None,
                        }
                    }
                    Some(ex) => {
                        let mut diff = Map::new();
                        let cur_folder = crate::output::field(&ex, "milestone_id").and_then(Value::as_u64);
                        let want_folder = dest.as_ref().and_then(id);
                        if cur_folder != want_folder {
                            x.change(
                                "→",
                                tr!(
                                    format!("move « {} » → {dest_title}", e.title),
                                    format!("déplacer « {} » → {dest_title}", e.title)
                                ),
                            );
                            if let Some(cf) = cur_folder {
                                *moved_out.entry(cf).or_default() += 1;
                            }
                            if let Some(w) = want_folder {
                                diff.insert("milestone_id".into(), json!(w));
                            }
                        }
                        let mut fields = Vec::new();
                        for (k, v) in &item {
                            let same = match k.as_str() {
                                "start_date" | "end_date" => lgu::same_time(s(&ex, k), v.as_str().unwrap_or(""), &x.tz),
                                "duration" => {
                                    ex.get(k)
                                        .and_then(|d| d.as_i64().or_else(|| d.as_str().and_then(|s| s.parse().ok())))
                                        == v.as_i64()
                                }
                                _ => ex.get(k) == Some(v),
                            };
                            if !same {
                                diff.insert(k.clone(), v.clone());
                                if k != "external_uuid" {
                                    fields.push(k.clone());
                                }
                            }
                        }
                        if !fields.is_empty() {
                            x.change(
                                "~",
                                tr!(
                                    format!("experiment « {} »: {fields:?}", e.title),
                                    format!("expérience « {} » : {fields:?}", e.title)
                                ),
                            );
                        }
                        if o.apply && !diff.is_empty() {
                            c.put(&format!("experiments/{}", id(&ex).unwrap_or(0)), Value::Object(diff))?;
                        }
                        Some(ex)
                    }
                };
                let texts: Vec<(String, String)> = e.sections.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
                x.fill("experiments", exp.as_ref(), &texts, &root.join(&f.dir), &e.title)?;
            }
        }

        // status folders that became empty are deleted (never an experiment)
        if by_status && let Some(p) = pid {
            for st in [Status::Done, Status::InProgress, Status::Planned] {
                if groups.contains_key(&st) {
                    continue;
                }
                let Some(g) = x.find("milestones", &format!("{fuuid}#{}", st.key()), &label(st), Some(p))? else {
                    continue;
                };
                let gid = id(&g).unwrap_or(0);
                let inside = x
                    .list("experiments")?
                    .iter()
                    .filter(|e| crate::output::field(e, "milestone_id").and_then(Value::as_u64) == Some(gid))
                    .count();
                let left = if o.apply {
                    c.get(&format!("milestones/{gid}"))?
                        .get("experiments")
                        .and_then(Value::as_array)
                        .map_or(0, |a| a.len())
                } else {
                    inside.saturating_sub(moved_out.get(&gid).copied().unwrap_or(0))
                };
                if left == 0 {
                    x.change(
                        "-",
                        tr!(
                            format!("empty folder « {} »", s(&g, "title")),
                            format!("dossier vide « {} »", s(&g, "title"))
                        ),
                    );
                    if o.apply
                        && let Err(e) = c.delete(&format!("milestones/{gid}"))
                    {
                        x.warn(tr!(
                            format!("could not delete folder {gid} ({e}); delete it in the web page"),
                            format!("dossier {gid} non supprimé ({e}) ; supprimez-le dans la page web")
                        ));
                    }
                }
            }
        }
    }

    // notes
    let notes_dir = root.join("notes");
    if notes_dir.is_dir() {
        let mut files: Vec<PathBuf> = std::fs::read_dir(&notes_dir)?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "md"))
            .collect();
        files.sort();
        for f in files {
            let t = read(&f)?;
            let title = entry::title_of(&t).unwrap_or_else(|| f.file_stem().unwrap().to_string_lossy().into_owned());
            let body = md::to_html(&entry::without_title(&t), &mut |_| None);
            let existing = match pid {
                Some(p) => x.find("notes", "", &title, Some(p))?,
                None => None,
            };
            match existing {
                None => {
                    x.change("+", tr!(format!("note « {title} »"), format!("note « {title} »")));
                    if o.apply {
                        x.create("notes", json!({"title": title, "body": body, "project_id": pid}))?;
                    }
                }
                Some(n) if s(&n, "body").split_whitespace().collect::<String>() != body.split_whitespace().collect::<String>() => {
                    x.change("~", tr!(format!("note « {title} »"), format!("note « {title} »")));
                    if o.apply {
                        c.put(&format!("notes/{}", id(&n).unwrap_or(0)), json!({"body": body}))?;
                    }
                }
                _ => {}
            }
        }
    }

    // Key Papers
    let bib = root.join("references.bib");
    if bib.is_file() {
        let pfull = pid.map(|p| c.get(&format!("projects/{p}"))).transpose()?;
        let mut lines = Vec::new();
        let n = crate::cmd::add_bib(c, &read(&bib)?, pfull.as_ref(), o.apply, &mut |l| lines.push(l))?;
        x.changes += n;
        for l in lines {
            (x.out)(l);
        }
    }

    // project attachments
    for dir in ["pieces", "attachments"] {
        let d = root.join(dir);
        if !d.is_dir() {
            continue;
        }
        let pfull = pid.map(|p| c.get(&format!("projects/{p}"))).transpose()?;
        let have: Vec<String> = pfull
            .as_ref()
            .and_then(|p| p.get("attachments"))
            .and_then(Value::as_array)
            .map(|a| a.iter().map(|x| s(x, "title").to_string()).collect())
            .unwrap_or_default();
        let mut files: Vec<PathBuf> = walkdir::WalkDir::new(&d)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
            .map(|e| e.into_path())
            .collect();
        files.sort();
        for f in files {
            let rel = f.strip_prefix(root).unwrap_or(&f).to_string_lossy().replace('\\', "/");
            if rel.ends_with(".gitkeep") || have.contains(&rel) {
                continue;
            }
            x.change("+", tr!(format!("attach {rel} to the project"), format!("joindre {rel} au projet")));
            if let (true, Some(p)) = (o.apply, &pfull) {
                c.upload(&f, Some(s(p, "uuid")), &rel)?;
            }
        }
    }

    Ok(Report {
        changes: x.changes,
        warnings: x.warnings,
    })
}

/// Reads Labguru back: differences = what a push would change, plus folder statuses as Labguru reports them.
pub fn check(c: &Client, root: &Path, o: PushOpts, out: &mut dyn FnMut(String)) -> Result<Report> {
    let cfg = load_config(root)?;
    let project = o.project.clone().unwrap_or(cfg.project.clone());
    let mut r = push(c, root, PushOpts { apply: false, ..o }, out)?;
    let Some(p) = c.all("projects", &[])?.into_iter().find(|x| s(x, "title") == project) else {
        return Ok(r);
    };
    let pid = id(&p);
    let folders = scan_folders(root)?;
    let all = c.all("milestones", &[])?;
    for f in &folders {
        let base = format!("lg:{project}/{}", f.dir);
        let mut want = vec![(base.clone(), folder_status(f))];
        if cfg.layout.style != "single" {
            let mut sts: Vec<Status> = f.entries.iter().map(|(_, e)| entry::group(e)).collect();
            sts.sort();
            sts.dedup();
            want.extend(sts.into_iter().map(|st| (format!("{base}#{}", st.key()), st)));
        }
        for (uuid, st) in want {
            let Some(m) = all
                .iter()
                .find(|m| s(m, "external_uuid") == uuid && crate::output::field(m, "project_id").and_then(Value::as_u64) == pid)
            else {
                continue;
            };
            let got = c.get(&format!("milestones/{}", id(m).unwrap_or(0)))?;
            let label = lgu::status_label(st.key());
            if s(&got, "status") != label && !s(&got, "status").is_empty() {
                r.changes += 1;
                out(tr!(
                    format!(
                        "≠ folder « {} »: Labguru says {}, notebook says {label}",
                        s(m, "title"),
                        s(&got, "status")
                    ),
                    format!(
                        "≠ dossier « {} » : Labguru indique {}, le cahier {label}",
                        s(m, "title"),
                        s(&got, "status")
                    )
                ));
            }
        }
    }
    Ok(r)
}

/// 1×1 transparent PNG for the example entry.
// 64×64: Labguru refuses a 1×1 image (HTTP 422)
const PNG: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52, 0x00, 0x00, 0x00, 0x40, 0x00, 0x00,
    0x00, 0x40, 0x08, 0x02, 0x00, 0x00, 0x00, 0x25, 0x0B, 0xE6, 0x89, 0x00, 0x00, 0x00, 0x4F, 0x49, 0x44, 0x41, 0x54, 0x78, 0xDA, 0xED,
    0xCF, 0x41, 0x09, 0x00, 0x00, 0x08, 0x04, 0xB0, 0x0B, 0x66, 0x30, 0x23, 0x1A, 0xCB, 0x08, 0xBE, 0x85, 0xC1, 0x0A, 0x2C, 0xD5, 0xF3,
    0x5A, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04,
    0x04, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04,
    0x04, 0x04, 0x04, 0x04, 0x2E, 0x0B, 0x04, 0x3F, 0xC1, 0x5A, 0x95, 0x64, 0x76, 0x0E, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44,
    0xAE, 0x42, 0x60, 0x82,
];

pub fn init(dir: &Path, project: Option<String>, host: Option<String>) -> Result<()> {
    if dir.join(CONFIG).exists() {
        bail!(tr!(
            format!("{} already has a {CONFIG}", dir.display()),
            format!("{} a déjà un {CONFIG}", dir.display())
        ));
    }
    let fr = crate::i18n::is_fr();
    let name = project.unwrap_or_else(|| {
        dir.file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Notebook".into())
    });
    let cfg = NbConfig {
        host,
        project: name.clone(),
        data_dirs: vec!["data".into()],
        layout: if fr { Layout::french() } else { Layout::default() },
    };
    let ex = if fr { "exemple" } else { "example" };
    let w = |p: &str, c: &str| -> Result<()> {
        let p = dir.join(p);
        std::fs::create_dir_all(p.parent().unwrap())?;
        std::fs::write(&p, c).with_context(|| p.display().to_string())
    };
    let head = tr!(
        "# lg notebook: `lg notebook push` (dry run), then `lg notebook push --apply`.\n# host: Labguru instance (default: the one you are logged in to).\n# data_dirs: where your data files live; their modification times give the real hours of each day.\n# layout.style: \"by-status\" (one folder per status) or \"single\" (one folder).\n",
        "# Cahier lg : `lg notebook push` (à blanc), puis `lg notebook push --apply`.\n# host : instance Labguru (par défaut : celle où vous êtes connecté).\n# data_dirs : dossiers de données ; leurs dates de modification donnent les heures réelles de chaque jour.\n# layout.style : \"by-status\" (un dossier par état) ou \"single\" (un seul dossier).\n"
    );
    w(CONFIG, &format!("{head}\n{}", toml::to_string_pretty(&cfg)?))?;
    if fr {
        w(
            "projet.md",
            &format!("# {name}\n\nBut du projet en quelques phrases : question, modèle, méthodes.\n"),
        )?;
        w(
            &format!("{ex}/README.md"),
            "# Exemple\n\nRésumé du dossier : ce que regroupent ces expériences.\n",
        )?;
        w(
            &format!("{ex}/2026-01-15-premier-essai.md"),
            "---\nstatut: terminée\ndébut: 2026-01-15\nfin: 2026-01-16\n---\n# Premier essai\n\n## Description\n\nCe que l'on cherche à savoir.\n\n## Procédure\n\n1. Préparer les échantillons (`tampon A`).\n2. Mesurer.\n\n| Échantillon | Valeur |\n|---|---|\n| A | 1,2 |\n\n## Résultats\n\nLe 16/01/2026 : mesure faite.\n\n![Figure](figures/exemple.png)\n",
        )?;
        w(
            &format!("{ex}/2026-02-01-essai-prevu.md"),
            "# Essai prévu\n\n## Description\n\nÀ faire.\n\n## Procédure\n\n## Résultats\n\nNon réalisé.\n",
        )?;
        w("notes/reunion.md", "# Réunion d'équipe\n\nDécisions et points ouverts.\n")?;
    } else {
        w(
            "project.md",
            &format!("# {name}\n\nAim of the project in a few sentences: question, model, methods.\n"),
        )?;
        w(
            &format!("{ex}/README.md"),
            "# Example\n\nFolder summary: what these experiments have in common.\n",
        )?;
        w(
            &format!("{ex}/2026-01-15-first-run.md"),
            "---\nstatus: done\nstart: 2026-01-15\nend: 2026-01-16\n---\n# First run\n\n## Description\n\nWhat we want to know.\n\n## Procedure\n\n1. Prepare the samples (`buffer A`).\n2. Measure.\n\n| Sample | Value |\n|---|---|\n| A | 1.2 |\n\n## Results\n\n2026-01-16: measured.\n\n![Figure](figures/example.png)\n",
        )?;
        w(
            &format!("{ex}/2026-02-01-planned-run.md"),
            "# Planned run\n\n## Description\n\nTo do.\n\n## Procedure\n\n## Results\n\nNot done.\n",
        )?;
        w("notes/meeting.md", "# Team meeting\n\nDecisions and open points.\n")?;
    }
    let fig = dir.join(ex).join("figures").join(format!("{ex}.png"));
    std::fs::create_dir_all(fig.parent().unwrap())?;
    std::fs::write(fig, PNG)?;
    w(
        "references.bib",
        "@article{watson1953,\n  author = {Watson, James D. and Crick, Francis H. C.},\n  title = {Molecular Structure of Nucleic Acids},\n  journal = {Nature},\n  year = {1953},\n  volume = {171},\n  pages = {737--738},\n  doi = {10.1038/171737a0}\n}\n",
    )?;
    w(if fr { "pieces/.gitkeep" } else { "attachments/.gitkeep" }, "")?;
    w("data/.gitkeep", "")?;
    eprintln!(
        "✓ {}",
        tr!(
            format!(
                "Notebook created in {}. Next: cd {} && lg notebook push",
                dir.display(),
                dir.display()
            ),
            format!(
                "Cahier créé dans {}. Ensuite : cd {} && lg notebook push",
                dir.display(),
                dir.display()
            )
        )
    );
    Ok(())
}

pub fn slug(t: &str) -> String {
    let s: String = t
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect();
    s.split('-').filter(|p| !p.is_empty()).collect::<Vec<_>>().join("-")
}

pub fn new_entry(root: &Path, folder: &str, title: &str) -> Result<PathBuf> {
    if folder.contains(['/', '\\']) || folder.starts_with('.') {
        bail!(tr!("folder: a single directory name", "dossier : un seul nom de répertoire"));
    }
    let d = root.join(folder);
    std::fs::create_dir_all(&d)?;
    let fr = crate::i18n::is_fr();
    if !d.join("README.md").exists() {
        std::fs::write(
            d.join("README.md"),
            format!("# {folder}\n\n{}\n", tr!("Folder summary.", "Résumé du dossier.")),
        )?;
    }
    let p = d.join(format!("{}.md", slug(title)));
    if p.exists() {
        bail!(tr!(
            format!("{} already exists", p.display()),
            format!("{} existe déjà", p.display())
        ));
    }
    let body = if fr {
        format!("---\nstatut: prévue\n---\n# {title}\n\n## Description\n\n## Procédure\n\n## Résultats\n")
    } else {
        format!("---\nstatus: planned\n---\n# {title}\n\n## Description\n\n## Procedure\n\n## Results\n")
    };
    std::fs::write(&p, body)?;
    Ok(p)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs() {
        assert_eq!(slug("Library QC — run 2!"), "library-qc-run-2");
        assert_eq!(slug("Évaluation"), "évaluation");
    }
}
