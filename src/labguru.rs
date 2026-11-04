//! Labguru object helpers shared by the commands and the notebook: sections and text blocks,
//! folder status, dates, image URLs, papers, tags.

use crate::api::Client;
use anyhow::Result;
use serde_json::{Value, json};

pub const COMMENT_MARK: &str = "lg-comment";

/// Sections of an experiment, folder or project (experiments wrap each one in "experiment_procedure").
pub fn sections(obj: &Value) -> Vec<Value> {
    obj.get("experiment_procedures")
        .and_then(Value::as_array)
        .map(|a| a.iter().map(|s| s.get("experiment_procedure").unwrap_or(s).clone()).collect())
        .unwrap_or_default()
}

pub fn id(v: &Value) -> Option<u64> {
    v.get("id")
        .and_then(|x| x.as_u64().or_else(|| x.as_str().and_then(|s| s.parse().ok())))
}

pub fn s<'a>(v: &'a Value, k: &str) -> &'a str {
    v.get(k).and_then(Value::as_str).unwrap_or("")
}

#[derive(Debug, PartialEq)]
pub enum TextOutcome {
    Unchanged,
    Updated,
    Created,
    /// Holds inline comments from the team: never overwritten.
    Commented(u64),
}

/// Equal as Labguru stores it: whitespace ignored, `<br />` stored as `<br>`, `&quot;` `&#39;` `&amp;` decoded.
pub fn same_html(a: &str, b: &str) -> bool {
    let n = |s: &str| {
        s.split_whitespace()
            .collect::<String>()
            .replace("/>", ">")
            .replace("&quot;", "\"")
            .replace("&#39;", "'")
            .replace("&amp;", "&")
    };
    n(a) == n(b)
}

/// Text block of a section that lg would write to: the first text element.
pub fn text_element(section: &Value) -> Option<Value> {
    section
        .get("elements")?
        .as_array()?
        .iter()
        .find(|e| s(e, "element_type") == "text")
        .cloned()
}

/// What `put_text` would do, without writing.
pub fn text_plan(c: &Client, section: &Value, html: &str) -> Result<TextOutcome> {
    let Some(el) = text_element(section) else {
        return Ok(TextOutcome::Created);
    };
    let eid = id(&el).unwrap_or(0);
    let data = match el.get("data").and_then(Value::as_str) {
        Some(d) => d.to_string(),
        None => s(&c.get(&format!("elements/{eid}"))?, "data").to_string(),
    };
    Ok(if data.contains(COMMENT_MARK) {
        TextOutcome::Commented(eid)
    } else if same_html(&data, html) {
        TextOutcome::Unchanged
    } else {
        TextOutcome::Updated
    })
}

/// Fills a section's text block (sections start with an empty one), or adds one.
/// PUT /elements/{id} takes `data` at the root, not in `item`.
pub fn put_text(c: &Client, section: &Value, html: &str) -> Result<TextOutcome> {
    let plan = text_plan(c, section, html)?;
    match plan {
        TextOutcome::Updated => {
            let eid = text_element(section).and_then(|e| id(&e)).unwrap_or(0);
            let mut top = serde_json::Map::new();
            top.insert("data".into(), json!(html));
            c.send("PUT", &format!("elements/{eid}"), None, top)?;
        }
        TextOutcome::Created => {
            c.post(
                "elements",
                json!({"container_id": id(section), "container_type": "ExperimentProcedure", "element_type": "text", "data": html}),
            )?;
        }
        _ => {}
    }
    Ok(plan)
}

/// POST /sections on any container (container_type = its class_name), then fetch it with its elements.
pub fn add_section(c: &Client, container: &Value, name: &str) -> Result<Value> {
    let sec = c.post(
        "sections",
        json!({"name": name, "container_id": id(container), "container_type": s(container, "class_name")}),
    )?;
    let sid = id(&sec).unwrap_or(0);
    c.get(&format!("sections/{sid}"))
}

/// Status of a folder, as the three fields Labguru reads.
pub fn folder_status(status: &str, completion: Option<&str>) -> Value {
    match status {
        "completed" => {
            json!({"completed": true, "current": false, "compleation_date": completion.map(|d| d.to_string()).unwrap_or_else(|| jiff::Zoned::now().date().to_string())})
        }
        "in-progress" => json!({"completed": false, "current": true, "compleation_date": null}),
        _ => json!({"completed": false, "current": false, "compleation_date": null}),
    }
}

/// Label Labguru reports in the "status" field of a folder.
pub fn status_label(status: &str) -> &'static str {
    match status {
        "completed" => "Completed",
        "in-progress" => "In Progress",
        _ => "Planned",
    }
}

/// Full-size image URL: only the /user_assets/…/original/<file> path displays (unlisted_url is empty,
/// thumb is pixelated).
pub fn original_url(a: &Value) -> String {
    let t = s(a, "thumb_url");
    match t.split_once("/thumb/") {
        Some((base, _)) => {
            let name = s(a, "attachment_file_name");
            let enc: String = name
                .bytes()
                .map(|b| {
                    if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
                        (b as char).to_string()
                    } else {
                        format!("%{b:02X}")
                    }
                })
                .collect();
            format!("{base}/original/{enc}")
        }
        None => a
            .get("url")
            .and_then(Value::as_str)
            .map(String::from)
            .unwrap_or_else(|| format!("/system/attachments/{}", id(a).unwrap_or(0))),
    }
}

/// "2026-10-19" → local midnight in UTC; timestamps → UTC; "2026-10-19T09:00" → local time in UTC.
pub fn iso_utc(s: &str, tz: &jiff::tz::TimeZone) -> Result<String> {
    if let Ok(t) = s.parse::<jiff::Timestamp>() {
        return Ok(t.to_string());
    }
    if let Ok(dt) = s.parse::<jiff::civil::DateTime>() {
        return Ok(dt.to_zoned(tz.clone())?.timestamp().to_string());
    }
    let d: jiff::civil::Date = s.parse().map_err(|_| {
        anyhow::anyhow!(tr!(
            format!("invalid date '{s}': use YYYY-MM-DD"),
            format!("date invalide « {s} » : utilisez AAAA-MM-JJ")
        ))
    })?;
    Ok(d.to_zoned(tz.clone())?.timestamp().to_string())
}

/// Same instant (or same local day when Labguru returns only a date).
pub fn same_time(labguru: &str, ours: &str, tz: &jiff::tz::TimeZone) -> bool {
    let inst = |x: &str| x.parse::<jiff::Timestamp>().ok();
    match (inst(labguru), inst(ours)) {
        (Some(a), Some(b)) => a.as_second() == b.as_second(),
        _ => {
            let day = |x: &str| {
                inst(x)
                    .map(|t| t.to_zoned(tz.clone()).date().to_string())
                    .unwrap_or_else(|| x.chars().take(10).collect())
            };
            day(labguru) == day(ours)
        }
    }
}

/// Inclusive number of days between two dates (Gantt duration).
pub fn days(start: jiff::civil::Date, end: jiff::civil::Date) -> i64 {
    (end - start).get_days() as i64 + 1
}

pub fn tag_names(obj: &Value) -> Vec<String> {
    obj.get("tags")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|t| {
                    t.as_str()
                        .map(String::from)
                        .or_else(|| t.get("name").or(t.get("tag")).and_then(Value::as_str).map(String::from))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Links a paper to a project (Key Papers) unless already linked.
pub fn link_paper(c: &Client, project: &Value, paper: &Value) -> Result<bool> {
    let puuid = s(paper, "uuid");
    let linked = project.get("links").and_then(Value::as_array).is_some_and(|l| {
        l.iter()
            .any(|x| x.as_str() == Some(puuid) || s(x, "target_uuid") == puuid || s(x, "uuid") == puuid)
    });
    if linked {
        return Ok(false);
    }
    c.post(
        "links",
        json!({
        "source_id": id(project), "source_type": "Projects::Project", "source_uuid": s(project, "uuid"),
        "target_id": id(paper), "target_type": "Knowledgebase::Paper", "target_uuid": puuid}),
    )?;
    Ok(true)
}

/// Existing paper with the same DOI, else the same title.
pub fn find_paper<'a>(papers: &'a [Value], item: &serde_json::Map<String, Value>) -> Option<&'a Value> {
    let doi = item.get("doi").and_then(Value::as_str).unwrap_or("").to_lowercase();
    let name = item.get("name").and_then(Value::as_str).unwrap_or("").to_lowercase();
    papers
        .iter()
        .find(|p| (!doi.is_empty() && s(p, "doi").to_lowercase() == doi) || (!name.is_empty() && s(p, "name").to_lowercase() == name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls_and_dates() {
        let a = json!({"id": 1, "attachment_file_name": "a b.png", "thumb_url": "/user_assets/6/attachments/1/thumb/a b.jpg?9"});
        assert_eq!(original_url(&a), "/user_assets/6/attachments/1/original/a%20b.png");
        assert_eq!(original_url(&json!({"id": 4})), "/system/attachments/4");
        let paris = jiff::tz::TimeZone::fixed(jiff::tz::offset(2));
        assert_eq!(iso_utc("2026-10-19", &paris).unwrap(), "2026-10-18T22:00:00Z");
        assert_eq!(iso_utc("2026-10-19T09:41:19+02:00", &paris).unwrap(), "2026-10-19T07:41:19Z");
        assert_eq!(iso_utc("2026-10-19T09:41:19", &paris).unwrap(), "2026-10-19T07:41:19Z");
        assert!(iso_utc("19 oct", &paris).is_err());
        assert!(same_time("2026-10-19T09:41:19.000+02:00", "2026-10-19T07:41:19Z", &paris));
        assert!(same_time("2026-10-19", "2026-10-18T22:00:00Z", &paris));
        assert!(!same_time("2026-10-20", "2026-10-18T22:00:00Z", &paris));
        assert_eq!(days(jiff::civil::date(2026, 8, 28), jiff::civil::date(2026, 11, 2)), 67);
        assert_eq!(folder_status("in-progress", None)["current"], true);
        assert_eq!(folder_status("completed", Some("2026-01-02"))["compleation_date"], "2026-01-02");
    }

    #[test]
    fn misc() {
        let e = json!({"experiment_procedures": [{"experiment_procedure": {"id": 3, "name": "Results"}}]});
        assert_eq!(sections(&e)[0]["name"], "Results");
        assert_eq!(tag_names(&json!({"tags": ["a", {"name": "b"}]})), ["a", "b"]);
        assert!(same_html("<p>a</p>\n", "<p>a</p>"));
        // as cle.inserm.fr stores it
        assert!(same_html(
            "<p>a &quot;b&quot; &amp; c<br />\nd</p><hr /><a href=\"/?a=1&amp;b=2\">x</a>",
            "<p>a \"b\" & c<br>\nd</p>\n<hr>\n<a href=\"/?a=1&b=2\">x</a>"
        ));
    }
}
