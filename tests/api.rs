mod fake;

use fake::{Fake, TOKEN};
use lg::api::{AuthRequired, Client};
use lg::notebook::{self, PushOpts};
use serde_json::json;
use std::path::PathBuf;

fn client(f: &Fake) -> Client {
    let mut c = Client::new(&f.url, TOKEN);
    c.waits = vec![0, 0, 0];
    c
}

fn tmpdir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("lg-test-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn retries_pagination_and_errors() {
    let f = Fake::start();
    let c = client(&f);
    for i in 0..5 {
        c.post("projects", json!({"title": format!("P{i}")})).unwrap();
    }
    assert_eq!(c.all("projects", &[]).unwrap().len(), 5); // 3 pages of 2

    f.state.lock().unwrap().fail_next = 2;
    assert!(c.get("projects/101").is_ok(), "two 503 then success");

    f.state.lock().unwrap().fail_next = 9;
    let e = c.get("projects/101").unwrap_err().to_string();
    assert!(e.contains("503") && !e.contains(TOKEN), "token must be masked: {e}");
    f.state.lock().unwrap().fail_next = 0;

    let bad = Client::new(&f.url, "wrong-token-xyz");
    let e = bad.get("projects").unwrap_err();
    assert!(e.downcast_ref::<AuthRequired>().is_some());

    // PUT with an empty body
    let m = c.post("milestones", json!({"title": "F", "project_id": 101})).unwrap();
    assert_eq!(c.put(&format!("milestones/{}", m["id"]), json!({"title": "G"})).unwrap(), json!({}));
    assert!(c.get("milestones/999").unwrap_err().to_string().contains("404"));
}

#[test]
fn upload_and_password_login() {
    let f = Fake::start();
    let c = client(&f);
    let p = c.post("projects", json!({"title": "P"})).unwrap();
    let d = tmpdir("upload");
    let file = d.join("a b.txt");
    std::fs::write(&file, "hello").unwrap();
    let a = c.upload(&file, p["uuid"].as_str(), "pieces/a b.txt").unwrap();
    assert_eq!(a["title"], "pieces/a b.txt");
    assert_eq!(
        c.get(&format!("projects/{}", p["id"])).unwrap()["attachments"][0]["attachment_file_name"],
        "a b.txt"
    );
    assert_eq!(lg::auth::token_from_password(&f.url, "me@x.org", "pw", None).unwrap(), TOKEN);
    let e = lg::auth::token_from_password(&f.url, "me@x.org", "nope", None).unwrap_err();
    assert!(e.downcast_ref::<AuthRequired>().is_some());
}

fn opts(apply: bool) -> PushOpts {
    PushOpts {
        apply,
        project: None,
        today: jiff::civil::date(2026, 11, 3),
        tz: jiff::tz::TimeZone::fixed(jiff::tz::offset(1)),
    }
}

fn run(c: &Client, root: &std::path::Path, apply: bool) -> (usize, Vec<String>) {
    let mut lines = Vec::new();
    let r = notebook::push(c, root, opts(apply), &mut |l| lines.push(l)).unwrap();
    (r.changes, lines)
}

#[test]
fn notebook_push_is_idempotent() {
    let f = Fake::start();
    let c = client(&f);
    let root = tmpdir("nb").join("cahier");
    notebook::init(&root, Some("Test project".into()), None).unwrap();
    let ex = if root.join("exemple").is_dir() { "exemple" } else { "example" };

    // dry run: reads only
    let (n, lines) = run(&c, &root, false);
    assert!(n > 5, "{lines:?}");
    assert!(f.writes().is_empty(), "dry run wrote: {:?}", f.writes());

    // apply
    let (n2, _) = run(&c, &root, true);
    assert!(n2 > 0);
    assert_eq!(f.count("projects"), 1);
    assert_eq!(f.count("experiments"), 2);
    assert_eq!(f.count("notes"), 1);
    assert_eq!(f.count("papers"), 1);
    // folder + completed group + planned group
    assert_eq!(f.count("milestones"), 3);
    let exps = c.all("experiments", &[]).unwrap();
    let first = exps
        .iter()
        .find(|e| {
            e["external_uuid"].as_str().unwrap().ends_with("15-premier-essai.md")
                || e["external_uuid"].as_str().unwrap().ends_with("15-first-run.md")
        })
        .unwrap();
    assert!(
        first["external_uuid"]
            .as_str()
            .unwrap()
            .starts_with(&format!("lg:Test project/{ex}/"))
    );
    assert_eq!(first["duration"], 2);
    assert_eq!(first["start_date"], "2026-01-14T23:00:00Z"); // local midnight, UTC+1
    // image attached and shown at full size
    let full = c.get(&format!("experiments/{}", first["id"])).unwrap();
    let html: String = full["experiment_procedures"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|s| s["experiment_procedure"]["elements"].as_array().unwrap().clone())
        .map(|e| e["data"].as_str().unwrap_or("").to_string())
        .collect();
    assert!(
        html.contains("/user_assets/1/attachments/") && html.contains("/original/"),
        "{html}"
    );
    assert!(html.contains("<table>") && html.contains("font-family: monospace"), "{html}");
    let folders = c.all("milestones", &[]).unwrap();
    assert!(folders.iter().all(|m| m["ancestry"].is_null()));

    // second push: nothing to do, nothing written
    f.clear_log();
    let (n3, lines) = run(&c, &root, true);
    assert_eq!(n3, 0, "{lines:?}");
    assert!(f.writes().is_empty(), "{:?}", f.writes());
    assert_eq!(f.count("experiments"), 2);
    let mut out = Vec::new();
    assert_eq!(
        notebook::check(&c, &root, opts(false), &mut |l| out.push(l)).unwrap().changes,
        0,
        "{out:?}"
    );

    // the planned entry gets results → in progress: moved, the empty "planned" folder deleted
    let planned = std::fs::read_dir(root.join(ex))
        .unwrap()
        .map(|e| e.unwrap().path())
        .find(|p| p.to_string_lossy().contains("2026-02-01"))
        .unwrap();
    let t = std::fs::read_to_string(&planned).unwrap();
    let res = if ex == "exemple" { "## Résultats" } else { "## Results" };
    std::fs::write(
        &planned,
        t.split(res).next().unwrap().to_string() + res + "\n\n02/11/2026: first pass.\n",
    )
    .unwrap();
    let (_, lines) = run(&c, &root, false);
    assert!(lines.iter().any(|l| l.starts_with('→')), "{lines:?}");
    assert!(lines.iter().any(|l| l.starts_with('-')), "{lines:?}");
    run(&c, &root, true);
    assert_eq!(f.count("experiments"), 2, "never deletes or duplicates experiments");
    assert_eq!(f.count("milestones"), 3); // planned folder gone, in-progress folder created
    assert_eq!(run(&c, &root, true).0, 0);

    // a block with inline comments is never overwritten
    {
        let mut s = f.state.lock().unwrap();
        for e in s.db.get_mut("elements").unwrap() {
            if e["data"].as_str().unwrap_or("").contains("first pass") {
                e["data"] = json!("<p>first pass <span class=\"lg-comment\">x</span></p>");
            }
        }
    }
    std::fs::write(
        &planned,
        std::fs::read_to_string(&planned).unwrap().replace("first pass", "second pass"),
    )
    .unwrap();
    f.clear_log();
    let mut lines = Vec::new();
    let r = notebook::push(&c, &root, opts(true), &mut |l| lines.push(l)).unwrap();
    assert_eq!(r.warnings, 1, "{lines:?}");
    assert!(
        !f.writes().iter().any(|w| w.starts_with("PUT /api/v1/elements")),
        "{:?}",
        f.writes()
    );
}
