//! Labguru API client. Wraps the v1 quirks once: token in the query for GET and in the JSON body
//! otherwise, `item` envelope, empty PUT responses, retries on 429/5xx, multipart uploads.

use crate::config::{Caps, base_url};
use anyhow::{Result, anyhow, bail};
use serde_json::{Map, Value, json};
use std::io::Read;
use std::path::Path;
use std::sync::Mutex;
use std::time::Duration;

/// Errors that map to exit code 4.
#[derive(Debug)]
pub struct AuthRequired(pub String);
impl std::fmt::Display for AuthRequired {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for AuthRequired {}

/// HTTP error with a status code (body already redacted).
#[derive(Debug)]
pub struct HttpError {
    pub status: u16,
    pub what: String,
    pub body: String,
}
impl std::fmt::Display for HttpError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let hint = match self.status {
            403 => tr!(
                " — your account may not have access to this object",
                " — votre compte n'a peut-être pas accès à cet objet"
            ),
            404 => tr!(
                " — check the id (lg <type> list) or the instance (lg auth status)",
                " — vérifiez l'identifiant (lg <type> list) ou l'instance (lg auth status)"
            ),
            405 | 501 => tr!(" — this instance does not support it", " — cette instance ne le permet pas"),
            422 => tr!(" — Labguru refused the values", " — Labguru a refusé les valeurs"),
            _ => "",
        };
        write!(f, "HTTP {} {}{hint}", self.status, self.what)?;
        if !self.body.trim().is_empty() {
            write!(f, ": {}", self.body.trim())?;
        }
        Ok(())
    }
}
impl std::error::Error for HttpError {}

static SECRETS: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// Remember a secret so that `redact` hides it from every message.
pub fn register_secret(s: &str) {
    if s.len() >= 4 {
        SECRETS.lock().unwrap().push(s.to_string());
    }
}

pub fn redact(s: &str) -> String {
    let mut out = s.to_string();
    for t in SECRETS.lock().unwrap().iter() {
        out = out.replace(t.as_str(), "***");
        // also the form it takes once URL-encoded
        let enc: String = t
            .bytes()
            .map(|b| {
                if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
                    (b as char).to_string()
                } else {
                    format!("%{b:02X}")
                }
            })
            .collect();
        out = out.replace(&enc, "***");
    }
    out
}

pub struct Client {
    pub host: String,
    pub base: String,
    token: String,
    agent: ureq::Agent,
    pub caps: Option<Caps>,
    /// Seconds to wait before each retry on 429/5xx.
    pub waits: Vec<u64>,
}

pub fn agent(timeout: u64) -> ureq::Agent {
    ureq::Agent::config_builder()
        .http_status_as_error(false)
        .timeout_global(Some(Duration::from_secs(timeout)))
        .user_agent(concat!("lg/", env!("CARGO_PKG_VERSION")))
        .build()
        .into()
}

/// "projects" → "/api/v1/projects.json"; "v2/experiments" → "/api/v2/experiments"; "/api/v1/x" kept.
pub fn api_path(p: &str) -> String {
    let p = p.trim_start_matches('/');
    let p = p.strip_prefix("api/").unwrap_or(p);
    let (ver, rest) = match p.split_once('/') {
        Some((v, r)) if v.len() >= 2 && v.starts_with('v') && v[1..].chars().all(|c| c.is_ascii_digit()) => (v, r),
        _ => ("v1", p),
    };
    let (rest, q) = rest.split_once('?').map_or((rest, None), |(a, b)| (a, Some(b)));
    let last = rest.rsplit('/').next().unwrap_or("");
    let json = if ver == "v1" && !last.contains('.') { ".json" } else { "" };
    match q {
        Some(q) => format!("/api/{ver}/{rest}{json}?{q}"),
        None => format!("/api/{ver}/{rest}{json}"),
    }
}

/// Swagger template of a concrete path, for capability checks: "/api/v1/experiments/12.json" matches
/// "/api/v1/experiments/{id}".
pub fn path_matches(template: &str, path: &str) -> bool {
    let path = path.split('?').next().unwrap_or(path).trim_end_matches(".json");
    let (a, b): (Vec<_>, Vec<_>) = (template.split('/').collect(), path.split('/').collect());
    a.len() == b.len() && a.iter().zip(&b).all(|(t, p)| t == p || (t.starts_with('{') && t.ends_with('}')))
}

impl Client {
    pub fn new(host: &str, token: &str) -> Client {
        register_secret(token);
        Client {
            host: host.to_string(),
            base: base_url(host),
            token: token.to_string(),
            agent: agent(120),
            caps: None,
            waits: vec![5, 30, 120],
        }
    }

    /// Ok when the instance documents this endpoint (or when its documentation is unknown).
    pub fn supports(&self, method: &str, path: &str) -> bool {
        let p = api_path(path);
        match &self.caps {
            Some(c) if !c.endpoints.is_empty() => c.endpoints.iter().any(|e| {
                e.split_once(' ')
                    .is_some_and(|(m, t)| m.eq_ignore_ascii_case(method) && path_matches(t, &p))
            }),
            _ => true,
        }
    }

    pub fn require(&self, method: &str, path: &str, what: &str) -> Result<()> {
        if self.supports(method, path) {
            return Ok(());
        }
        bail!(tr!(
            format!(
                "{what} is not supported by the API of {} (not in its /api-docs). You can still try: lg api -X {method} {path}",
                self.host
            ),
            format!(
                "{what} : l'API de {} ne le permet pas (absent de ses /api-docs). Vous pouvez quand même essayer : lg api -X {method} {path}",
                self.host
            )
        ))
    }

    pub fn get(&self, path: &str) -> Result<Value> {
        self.request("GET", path, &[], None)
    }

    pub fn get_q(&self, path: &str, query: &[(String, String)]) -> Result<Value> {
        self.request("GET", path, query, None)
    }

    /// POST/PUT/DELETE with `{"token", "item": item, ..top}`.
    pub fn send(&self, method: &str, path: &str, item: Option<Value>, top: Map<String, Value>) -> Result<Value> {
        let mut body = top;
        if let Some(i) = item {
            body.insert("item".into(), i);
        }
        self.request(method, path, &[], Some(Value::Object(body)))
    }

    pub fn post(&self, path: &str, item: Value) -> Result<Value> {
        self.send("POST", path, Some(item), Map::new())
    }

    pub fn put(&self, path: &str, item: Value) -> Result<Value> {
        self.send("PUT", path, Some(item), Map::new())
    }

    pub fn delete(&self, path: &str) -> Result<Value> {
        self.send("DELETE", path, None, Map::new())
    }

    /// Raw request; the token is added here and nowhere else.
    pub fn request(&self, method: &str, path: &str, query: &[(String, String)], body: Option<Value>) -> Result<Value> {
        if !["GET", "POST", "PUT", "PATCH", "DELETE"].contains(&method.to_ascii_uppercase().as_str()) {
            bail!(tr!(
                format!("unknown HTTP method {method}"),
                format!("méthode HTTP inconnue : {method}")
            ));
        }
        let p = api_path(path);
        let url = format!("{}{}", self.base, p);
        let shown = format!("{method} {}", p.split('?').next().unwrap_or(&p));
        if std::env::var_os("LG_DEBUG").is_some() {
            eprintln!("> {shown}");
        }
        let mut waits = self.waits.iter();
        loop {
            let resp = self.call(method, &url, query, body.as_ref());
            let mut resp = match resp {
                Ok(r) => r,
                Err(e) => bail!(redact(&format!("{shown}: {e}"))),
            };
            let status = resp.status().as_u16();
            let text = resp.body_mut().with_config().limit(1 << 30).read_to_string().unwrap_or_default();
            if (status == 429 || status >= 500)
                && let Some(w) = waits.next()
            {
                eprintln!(
                    "{}",
                    tr!(
                        format!("  … HTTP {status}, retrying in {w} s"),
                        format!("  … HTTP {status}, nouvel essai dans {w} s")
                    )
                );
                std::thread::sleep(Duration::from_secs(*w));
                continue;
            }
            return self.finish(status, &shown, &text);
        }
    }

    fn finish(&self, status: u16, shown: &str, text: &str) -> Result<Value> {
        if status == 401 {
            return Err(AuthRequired(tr!(
                format!("{} refused the token. Run: lg auth login --hostname {}", self.host, self.host),
                format!("{} a refusé le token. Lancez : lg auth login --hostname {}", self.host, self.host)
            ))
            .into());
        }
        if !(200..300).contains(&status) {
            let body: String = redact(text).chars().take(300).collect();
            return Err(HttpError {
                status,
                what: shown.to_string(),
                body,
            }
            .into());
        }
        if text.trim().is_empty() {
            return Ok(json!({})); // some PUT answer with an empty body
        }
        serde_json::from_str(text).map_err(|_| {
            anyhow!(redact(&format!(
                "{shown}: not JSON: {}",
                text.chars().take(200).collect::<String>()
            )))
        })
    }

    fn call(
        &self,
        method: &str,
        url: &str,
        query: &[(String, String)],
        body: Option<&Value>,
    ) -> Result<ureq::http::Response<ureq::Body>, ureq::Error> {
        let a = &self.agent;
        let q = query.iter().map(|(k, v)| (k.as_str(), v.as_str()));
        if method.eq_ignore_ascii_case("GET") {
            return a.get(url).query("token", &self.token).query_pairs(q).call();
        }
        let mut b = body.cloned().unwrap_or_else(|| json!({}));
        if let Value::Object(m) = &mut b {
            m.insert("token".into(), Value::String(self.token.clone()));
        }
        let s = serde_json::to_string(&b).unwrap_or_default();
        let ct = "application/json";
        match method.to_ascii_uppercase().as_str() {
            "POST" => a.post(url).query_pairs(q).header("Content-Type", ct).send(&s),
            "PUT" => a.put(url).query_pairs(q).header("Content-Type", ct).send(&s),
            "PATCH" => a.patch(url).query_pairs(q).header("Content-Type", ct).send(&s),
            "DELETE" => a.delete(url).query_pairs(q).header("Content-Type", ct).force_send_body().send(&s),
            _ => a.get(url).query("token", &self.token).query_pairs(q).call(),
        }
    }

    /// Every page of a list endpoint (page=1, 2, … until an empty page).
    pub fn all(&self, path: &str, query: &[(String, String)]) -> Result<Vec<Value>> {
        let mut out = Vec::new();
        let mut prev: Option<Value> = None;
        for page in 1.. {
            let mut q = query.to_vec();
            q.push(("page".into(), page.to_string()));
            let batch = match self.get_q(path, &q)? {
                Value::Array(a) => a,
                other => return Ok(if out.is_empty() { vec![other] } else { out }),
            };
            // an endpoint that ignores `page` would loop forever
            if batch.is_empty() || prev.as_ref() == batch.first() {
                break;
            }
            prev = batch.first().cloned();
            out.extend(batch);
        }
        Ok(out)
    }

    /// Multipart upload to /attachments, streamed (large files never sit in memory).
    pub fn upload(&self, file: &Path, attach_to_uuid: Option<&str>, title: &str) -> Result<Value> {
        let name = file.file_name().and_then(|n| n.to_str()).unwrap_or("file").replace('"', "");
        let boundary = format!(
            "lg{:x}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        );
        let mut fields = vec![("token", self.token.as_str()), ("item[title]", title)];
        if let Some(u) = attach_to_uuid {
            fields.push(("item[attach_to_uuid]", u));
        }
        let mut head: String = fields
            .iter()
            .map(|(k, v)| format!("--{boundary}\r\nContent-Disposition: form-data; name=\"{k}\"\r\n\r\n{v}\r\n"))
            .collect();
        head += &format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"item[attachment]\"; filename=\"{name}\"\r\nContent-Type: {}\r\n\r\n",
            mime(&name)
        );
        let tail = format!("\r\n--{boundary}--\r\n");
        let f = std::fs::File::open(file).map_err(|e| anyhow!("{}: {e}", file.display()))?;
        let size = head.len() as u64 + f.metadata()?.len() + tail.len() as u64;
        let reader = std::io::Cursor::new(head.into_bytes())
            .chain(f)
            .chain(std::io::Cursor::new(tail.into_bytes()));
        let url = format!("{}{}", self.base, api_path("attachments"));
        let resp = agent(3600)
            .post(&url)
            .header("Content-Type", format!("multipart/form-data; boundary={boundary}"))
            .header("Content-Length", size.to_string())
            .send(ureq::SendBody::from_owned_reader(reader));
        let mut resp = resp.map_err(|e| anyhow!(redact(&format!("POST /api/v1/attachments: {e}"))))?;
        let status = resp.status().as_u16();
        let text = resp.body_mut().read_to_string().unwrap_or_default();
        self.finish(status, "POST /api/v1/attachments", &text)
    }

    /// GET an absolute or host-relative URL (attachment files) with the token, into a writer.
    pub fn download(&self, url: &str, out: &mut dyn std::io::Write) -> Result<u64> {
        let full = if url.starts_with("http") {
            url.to_string()
        } else {
            format!("{}{url}", self.base)
        };
        let same_host = full.starts_with(&self.base);
        let req = self.agent.get(&full);
        let req = if same_host { req.query("token", &self.token) } else { req }; // never send the token elsewhere
        let mut resp = req.call().map_err(|e| anyhow!(redact(&format!("GET {url}: {e}"))))?;
        let status = resp.status().as_u16();
        if !(200..300).contains(&status) {
            let shown = format!("GET {}", url.split('?').next().unwrap_or(url));
            return self.finish(status, &shown, "").map(|_| 0);
        }
        Ok(std::io::copy(&mut resp.body_mut().as_reader(), out)?)
    }
}

pub fn mime(name: &str) -> &'static str {
    let ext = name.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "svg" => "image/svg+xml",
        "webp" => "image/webp",
        "pdf" => "application/pdf",
        "csv" => "text/csv",
        "txt" | "md" | "bib" => "text/plain",
        "html" | "htm" => "text/html",
        "json" => "application/json",
        "zip" => "application/zip",
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        "pptx" => "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        _ => "application/octet-stream",
    }
}

/// Endpoints listed in an OpenAPI YAML ("  \"/api/v1/x\":" then "    get:" lines).
pub fn parse_swagger(yaml: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur: Option<&str> = None;
    for line in yaml.lines() {
        let top = line
            .strip_prefix("  ")
            .filter(|l| !l.starts_with(' '))
            .and_then(|l| l.strip_suffix(':'));
        if let Some(p) = top.map(|l| l.trim_matches('"')).filter(|l| l.starts_with('/')) {
            cur = Some(p);
        } else if !line.starts_with("  ") || top.is_some() {
            cur = None;
        } else if let (Some(p), Some(m)) = (cur, line.strip_prefix("    ").and_then(|l| l.strip_suffix(':')))
            && ["get", "post", "put", "delete", "patch"].contains(&m)
        {
            out.push(format!("{} {p}", m.to_ascii_uppercase()));
        }
    }
    out
}

/// Reads /api-docs/v1 and v2 of the instance (no token needed).
pub fn detect_caps(host: &str) -> Caps {
    let a = agent(30);
    let fetch = |v: &str| -> Option<String> {
        let mut r = a.get(format!("{}/api-docs/{v}/swagger.yaml", base_url(host))).call().ok()?;
        if r.status().as_u16() != 200 {
            return None;
        }
        let s = r.body_mut().with_config().limit(50 << 20).read_to_string().ok()?;
        s.contains("paths:").then_some(s)
    };
    let (v1, v2) = (fetch("v1"), fetch("v2"));
    let mut endpoints: Vec<String> = [&v1, &v2].iter().filter_map(|s| s.as_deref()).flat_map(parse_swagger).collect();
    endpoints.sort();
    endpoints.dedup();
    Caps {
        checked: jiff::Timestamp::now().to_string(),
        v1: v1.is_some() || endpoints.is_empty(),
        v2: v2.is_some(),
        endpoints,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths() {
        assert_eq!(api_path("projects"), "/api/v1/projects.json");
        assert_eq!(api_path("/api/v1/experiments/12"), "/api/v1/experiments/12.json");
        assert_eq!(api_path("v2/experiments"), "/api/v2/experiments");
        assert_eq!(api_path("projects?page=2"), "/api/v1/projects.json?page=2");
        assert_eq!(api_path("projects.json"), "/api/v1/projects.json");
        assert!(path_matches("/api/v1/experiments/{id}", "/api/v1/experiments/12.json"));
        assert!(!path_matches("/api/v1/experiments/{id}", "/api/v1/experiments.json"));
    }

    #[test]
    fn swagger() {
        let y = "openapi: 3.0.0\npaths:\n  \"/api/v1/sessions\":\n    post:\n      summary: x\n  \"/api/v1/projects/{id}\":\n    get:\n      tags:\n      - get:\n    put:\ncomponents:\n  schemas:\n    get:\n";
        assert_eq!(
            parse_swagger(y),
            ["POST /api/v1/sessions", "GET /api/v1/projects/{id}", "PUT /api/v1/projects/{id}"]
        );
        let mut c = Client::new("h", "secret-token-1");
        c.caps = Some(Caps {
            endpoints: parse_swagger(y),
            ..Default::default()
        });
        assert!(c.supports("GET", "projects/3") && !c.supports("DELETE", "projects/3"));
        assert!(c.require("DELETE", "projects/3", "x").is_err());
    }

    #[test]
    fn redaction() {
        register_secret("s3cr+t/tok");
        assert_eq!(redact("GET /x?token=s3cr%2Bt%2Ftok failed"), "GET /x?token=*** failed");
        assert_eq!(redact("body s3cr+t/tok"), "body ***");
    }
}
