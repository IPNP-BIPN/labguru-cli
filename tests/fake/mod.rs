//! A small in-memory Labguru v1 on 127.0.0.1, enough for the client and `notebook push`.

use serde_json::{Map, Value, json};
use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

pub const TOKEN: &str = "tok-SECRET-123";

#[derive(Default)]
pub struct State {
    pub db: BTreeMap<String, Vec<Value>>,
    pub next: u64,
    pub log: Vec<String>,
    /// Answer 503 to the next N requests.
    pub fail_next: usize,
}

pub struct Fake {
    pub url: String,
    pub state: Arc<Mutex<State>>,
}

impl Fake {
    pub fn start() -> Fake {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", l.local_addr().unwrap());
        let state = Arc::new(Mutex::new(State {
            next: 100,
            ..Default::default()
        }));
        let st = state.clone();
        std::thread::spawn(move || {
            for s in l.incoming().flatten() {
                let st = st.clone();
                std::thread::spawn(move || {
                    let _ = handle(s, &st);
                });
            }
        });
        Fake { url, state }
    }

    pub fn count(&self, kind: &str) -> usize {
        self.state.lock().unwrap().db.get(kind).map_or(0, |v| v.len())
    }

    pub fn writes(&self) -> Vec<String> {
        self.state
            .lock()
            .unwrap()
            .log
            .iter()
            .filter(|l| !l.starts_with("GET"))
            .cloned()
            .collect()
    }

    pub fn clear_log(&self) {
        self.state.lock().unwrap().log.clear();
    }
}

fn handle(s: std::net::TcpStream, st: &Mutex<State>) -> std::io::Result<()> {
    let mut r = BufReader::new(s.try_clone()?);
    loop {
        let mut line = String::new();
        if r.read_line(&mut line)? == 0 {
            return Ok(());
        }
        let mut parts = line.split_whitespace();
        let (method, target) = (parts.next().unwrap_or("").to_string(), parts.next().unwrap_or("").to_string());
        let mut len = 0usize;
        let mut ctype = String::new();
        loop {
            let mut h = String::new();
            r.read_line(&mut h)?;
            if h.trim().is_empty() {
                break;
            }
            let (k, v) = h.split_once(':').unwrap_or(("", ""));
            match k.to_ascii_lowercase().as_str() {
                "content-length" => len = v.trim().parse().unwrap_or(0),
                "content-type" => ctype = v.trim().to_string(),
                _ => {}
            }
        }
        let mut body = vec![0; len];
        r.read_exact(&mut body)?;
        let (status, out) = route(st, &method, &target, &ctype, &body);
        let text = if out.is_null() { String::new() } else { out.to_string() };
        write!(
            &s,
            "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{text}",
            text.len()
        )?;
    }
}

fn query(target: &str) -> BTreeMap<String, String> {
    target
        .split_once('?')
        .map(|(_, q)| {
            q.split('&')
                .filter_map(|kv| kv.split_once('='))
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect()
        })
        .unwrap_or_default()
}

fn multipart(body: &[u8]) -> (Map<String, Value>, String) {
    let text = String::from_utf8_lossy(body);
    let mut fields = Map::new();
    let mut filename = String::new();
    let re = regex::Regex::new(r#"name="([^"]+)"(?:; filename="([^"]+)")?[^\r]*\r\n(?:Content-Type: [^\r]*\r\n)?\r\n([^\r]*)"#).unwrap();
    for c in re.captures_iter(&text) {
        if let Some(f) = c.get(2) {
            filename = f.as_str().to_string();
        } else {
            fields.insert(c[1].to_string(), json!(c[3].to_string()));
        }
    }
    (fields, filename)
}

fn class(kind: &str) -> &'static str {
    match kind {
        "projects" => "Projects::Project",
        "milestones" => "Projects::Milestone",
        "experiments" => "Projects::Experiment",
        "notes" => "Knowledgebase::Note",
        "papers" => "Knowledgebase::Paper",
        _ => "Other",
    }
}

fn route(st: &Mutex<State>, method: &str, target: &str, ctype: &str, body: &[u8]) -> (u16, Value) {
    let mut s = st.lock().unwrap();
    let path = target.split('?').next().unwrap().to_string();
    s.log.push(format!("{method} {path}"));
    if s.fail_next > 0 {
        s.fail_next -= 1;
        return (503, json!({"error": format!("busy, your token is {TOKEN}")}));
    }
    if path.starts_with("/api-docs") {
        return (404, Value::Null);
    }
    let q = query(target);
    let (token, payload) = if ctype.starts_with("multipart") {
        let (f, name) = multipart(body);
        (
            f.get("token").and_then(Value::as_str).unwrap_or("").to_string(),
            json!({"fields": f, "filename": name}),
        )
    } else {
        let v: Value = serde_json::from_slice(body).unwrap_or(json!({}));
        let t = q
            .get("token")
            .cloned()
            .or_else(|| v.get("token").and_then(Value::as_str).map(String::from))
            .unwrap_or_default();
        (t, v)
    };
    let rest = path.trim_start_matches("/api/v1/").trim_end_matches(".json").to_string();
    if rest == "sessions" {
        return if payload["password"] == "pw" {
            (200, json!({"token": TOKEN}))
        } else {
            (401, json!({"error": "bad"}))
        };
    }
    if token != TOKEN {
        return (401, json!({"error": "unauthorized"}));
    }
    let seg: Vec<&str> = rest.split('/').collect();
    let kind = seg[0].to_string();
    let oid: Option<u64> = seg.get(1).and_then(|x| x.parse().ok());
    let item = payload.get("item").cloned().unwrap_or(json!({}));
    match (method, oid) {
        ("GET", None) => {
            let page: usize = q.get("page").and_then(|p| p.parse().ok()).unwrap_or(1);
            let rows = s.db.get(&kind).cloned().unwrap_or_default();
            let chunk: Vec<Value> = rows.chunks(2).nth(page - 1).map(|c| c.to_vec()).unwrap_or_default(); // 2 per page
            (200, Value::Array(chunk.iter().map(|r| view(&s, &kind, r)).collect()))
        }
        ("GET", Some(i)) => match find(&s, &kind, i) {
            Some(r) => (200, view(&s, &kind, &r)),
            None => (404, json!({"error": "not found"})),
        },
        ("POST", _) => post(&mut s, &kind, item, &payload),
        ("PUT", Some(i)) => {
            let data = if kind == "elements" {
                json!({"data": payload["data"]})
            } else {
                item
            };
            let Some(row) = s.db.get_mut(&kind).and_then(|v| v.iter_mut().find(|r| r["id"] == i)) else {
                return (404, json!({}));
            };
            for (k, v) in data.as_object().unwrap() {
                row[k] = v.clone();
            }
            // milestones answer with an empty body, like some real PUTs
            if kind == "milestones" {
                (200, Value::Null)
            } else {
                (200, row.clone())
            }
        }
        ("DELETE", Some(i)) => {
            if kind == "tags" {
                return (200, json!({}));
            }
            if let Some(v) = s.db.get_mut(&kind) {
                v.retain(|r| r["id"] != i);
            }
            (204, Value::Null)
        }
        _ => (404, json!({})),
    }
}

fn find(s: &State, kind: &str, i: u64) -> Option<Value> {
    s.db.get(kind)?.iter().find(|r| r["id"] == i).cloned()
}

fn by_uuid(s: &State, uuid: &str) -> Option<(String, u64)> {
    s.db.iter()
        .find_map(|(k, v)| v.iter().find(|r| r["uuid"] == uuid).map(|r| (k.clone(), r["id"].as_u64().unwrap())))
}

/// Object as Labguru returns it: sections with elements, attachments, experiments of a folder.
fn view(s: &State, kind: &str, r: &Value) -> Value {
    let mut v = r.clone();
    let id = r["id"].as_u64().unwrap_or(0);
    let class = class(kind);
    let secs: Vec<Value> =
        s.db.get("sections")
            .into_iter()
            .flatten()
            .filter(|x| x["container_id"] == id && x["container_type"] == class)
            .map(|x| section(s, x))
            .collect();
    if kind == "experiments" {
        v["experiment_procedures"] = Value::Array(secs.into_iter().map(|x| json!({"experiment_procedure": x})).collect());
    } else if kind != "sections" {
        v["experiment_procedures"] = Value::Array(secs);
    }
    if kind == "sections" {
        return section(s, r);
    }
    let uuid = r["uuid"].as_str().unwrap_or("");
    v["attachments"] = Value::Array(
        s.db.get("attachments")
            .into_iter()
            .flatten()
            .filter(|a| a["attach_to_uuid"] == uuid)
            .cloned()
            .collect(),
    );
    if kind == "milestones" {
        v["experiments"] = Value::Array(
            s.db.get("experiments")
                .into_iter()
                .flatten()
                .filter(|e| e["milestone_id"] == id)
                .map(|e| json!({"id": e["id"]}))
                .collect(),
        );
        let st = if r["completed"] == true {
            "Completed"
        } else if r["current"] == true {
            "In Progress"
        } else {
            "Planned"
        };
        v["status"] = json!(st);
    }
    v
}

fn section(s: &State, x: &Value) -> Value {
    let mut x = x.clone();
    let sid = x["id"].as_u64().unwrap();
    x["elements"] = Value::Array(
        s.db.get("elements")
            .into_iter()
            .flatten()
            .filter(|e| e["container_id"] == sid)
            .cloned()
            .collect(),
    );
    x
}

fn post(s: &mut State, kind: &str, item: Value, payload: &Value) -> (u16, Value) {
    s.next += 1;
    let id = s.next;
    if kind == "attachments" {
        let f = &payload["fields"];
        let name = payload["filename"].as_str().unwrap_or("f").to_string();
        let a = json!({"id": id, "title": f["item[title]"], "attach_to_uuid": f["item[attach_to_uuid]"], "attachment_file_name": name,
            "thumb_url": format!("/user_assets/1/attachments/{id}/thumb/{name}.jpg")});
        s.db.entry("attachments".into()).or_default().push(a.clone());
        return (201, a);
    }
    if kind == "tags" {
        return (201, json!({"tag": item["tag"]}));
    }
    if kind == "links" {
        let src = item["source_id"].as_u64().unwrap_or(0);
        if let Some(p) = s.db.get_mut("projects").and_then(|v| v.iter_mut().find(|p| p["id"] == src)) {
            p["links"].as_array_mut().unwrap().push(item["target_uuid"].clone());
        }
        return (201, json!({"id": id}));
    }
    if kind == "experiments" && item.get("milestone_id").is_none_or(Value::is_null) {
        return (422, json!({"error": "milestone_id is required"}));
    }
    let mut row = item.clone();
    row["id"] = json!(id);
    row["uuid"] = json!(format!("uuid-{id}"));
    row["class_name"] = json!(class(kind));
    if kind == "projects" {
        row["links"] = json!([]);
    }
    if kind == "milestones" {
        row.as_object_mut().unwrap().remove("description"); // never stored
        if row.get("ancestry").is_none() {
            row["ancestry"] = Value::Null;
        }
    }
    s.db.entry(kind.into()).or_default().push(row.clone());
    if kind == "experiments" {
        for n in ["Description", "Procedure", "Results"] {
            add_section(s, n, id, "Projects::Experiment");
        }
    }
    if kind == "sections" {
        s.db.get_mut("sections").unwrap().pop();
        let sec = add_section(
            s,
            item["name"].as_str().unwrap_or(""),
            item["container_id"].as_u64().unwrap_or(0),
            item["container_type"].as_str().unwrap_or(""),
        );
        return (201, sec);
    }
    if kind == "elements" {
        row["element_type"] = item["element_type"].clone();
    }
    let _ = by_uuid;
    (201, row)
}

fn add_section(s: &mut State, name: &str, container: u64, ctype: &str) -> Value {
    s.next += 1;
    let sid = s.next;
    let sec = json!({"id": sid, "name": name, "container_id": container, "container_type": ctype});
    s.db.entry("sections".into()).or_default().push(sec.clone());
    s.next += 1;
    let eid = s.next;
    s.db.entry("elements".into())
        .or_default()
        .push(json!({"id": eid, "container_id": sid, "element_type": "text", "data": ""}));
    sec
}
