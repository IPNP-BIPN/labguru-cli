//! A notebook entry: optional header (--- status/start/end ---), # title, ## Description / Procedure /
//! Results; its group (status) and its real dates.

use anyhow::{Result, bail};
use jiff::civil::Date;
use jiff::tz::TimeZone;
use jiff::{Timestamp, ToSpan};
use regex::Regex;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Status {
    Done,
    InProgress,
    Planned,
}

impl Status {
    /// Key used in folder status fields and external_uuid suffixes.
    pub fn key(self) -> &'static str {
        match self {
            Status::Done => "completed",
            Status::InProgress => "in-progress",
            Status::Planned => "planned",
        }
    }

    pub fn parse(s: &str) -> Option<Status> {
        Some(match s.trim().to_lowercase().as_str() {
            "terminée" | "terminé" | "terminées" | "terminés" | "terminee" | "done" | "completed" | "complete" => Status::Done,
            "en cours" | "in progress" | "in-progress" | "current" | "ongoing" => Status::InProgress,
            "prévue" | "prévu" | "prévues" | "prévus" | "prevue" | "planned" | "todo" => Status::Planned,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Default)]
pub struct Entry {
    pub title: String,
    pub status: Option<Status>,
    pub start: Option<Date>,
    pub end: Option<Date>,
    /// Labguru section name (Description, Procedure, Results) → Markdown.
    pub sections: BTreeMap<String, String>,
    /// Text without the header, for date scanning.
    pub text: String,
}

/// "2026-10-19" or "19/10/2026".
pub fn parse_date(s: &str) -> Option<Date> {
    let s = s.trim();
    if let Ok(d) = s.parse::<Date>() {
        return Some(d);
    }
    let p: Vec<&str> = s.split('/').collect();
    if p.len() == 3 {
        return Date::new(p[2].parse().ok()?, p[1].parse().ok()?, p[0].parse().ok()?).ok();
    }
    None
}

/// Splits on `## ` headings, ignoring those inside fenced code blocks.
pub fn split_md(text: &str) -> Vec<(String, String)> {
    let mut parts: Vec<(String, Vec<&str>)> = vec![(String::new(), vec![])];
    let mut fence = false;
    for line in text.lines() {
        if line.trim_start().starts_with("```") || line.trim_start().starts_with("~~~") {
            fence = !fence;
        }
        if !fence && let Some(h) = line.strip_prefix("## ") {
            parts.push((h.trim().to_string(), vec![]));
            continue;
        }
        parts.last_mut().unwrap().1.push(line);
    }
    parts.into_iter().map(|(h, l)| (h, l.join("\n").trim().to_string())).collect()
}

pub fn section_name(h: &str) -> Option<&'static str> {
    Some(match h.trim().to_lowercase().as_str() {
        "description" => "Description",
        "procédure" | "procedure" | "protocole" | "protocol" => "Procedure",
        "résultats" | "resultats" | "results" => "Results",
        _ => return None,
    })
}

/// Header between two `---` lines at the top of the file → (key/value pairs, rest).
pub fn front_matter(text: &str) -> (Vec<(String, String)>, &str) {
    let t = text.trim_start_matches('\u{feff}');
    let Some(rest) = t.strip_prefix("---\n").or_else(|| t.strip_prefix("---\r\n")) else {
        return (vec![], t);
    };
    let Some(end) = rest
        .find("\n---")
        .filter(|&i| rest[i + 4..].starts_with(['\n', '\r']) || rest.len() == i + 4)
    else {
        return (vec![], t);
    };
    let kv = rest[..end]
        .lines()
        .filter_map(|l| l.split_once(':'))
        .map(|(k, v)| (k.trim().to_lowercase(), v.trim().trim_matches('"').to_string()))
        .collect();
    let after = &rest[end + 4..];
    (kv, after.trim_start_matches(['\r', '\n']))
}

pub fn title_of(text: &str) -> Option<String> {
    text.lines().find_map(|l| l.strip_prefix("# ")).map(|t| t.trim().to_string())
}

/// Text without its "# title" line.
pub fn without_title(text: &str) -> String {
    let mut done = false;
    text.lines()
        .filter(|l| {
            if !done && l.starts_with("# ") {
                done = true;
                return false;
            }
            true
        })
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

pub fn parse_entry(text: &str, fallback_title: &str) -> Result<Entry> {
    let (kv, body) = front_matter(text);
    let mut e = Entry {
        title: title_of(body).unwrap_or_else(|| fallback_title.to_string()),
        text: body.to_string(),
        ..Default::default()
    };
    for (k, v) in kv {
        match k.as_str() {
            "statut" | "status" | "état" | "etat" => {
                e.status = Some(Status::parse(&v).ok_or_else(|| {
                    anyhow::anyhow!(tr!(
                        format!("unknown status '{v}': use done, in progress or planned"),
                        format!("statut inconnu « {v} » : utilisez terminée, en cours ou prévue")
                    ))
                })?)
            }
            "début" | "debut" | "start" => e.start = Some(parse_date(&v).ok_or_else(|| bad_date(&v))?),
            "fin" | "end" => e.end = Some(parse_date(&v).ok_or_else(|| bad_date(&v))?),
            _ => {}
        }
    }
    if let (Some(a), Some(b)) = (e.start, e.end)
        && b < a
    {
        bail!(tr!("end date is before start date", "la date de fin précède la date de début"));
    }
    for (h, c) in split_md(body) {
        if let Some(n) = section_name(&h)
            && !c.is_empty()
        {
            e.sections.insert(n.to_string(), c);
        }
    }
    Ok(e)
}

fn bad_date(v: &str) -> anyhow::Error {
    anyhow::anyhow!(tr!(
        format!("invalid date '{v}': use YYYY-MM-DD"),
        format!("date invalide « {v} » : utilisez AAAA-MM-JJ")
    ))
}

/// Header status; else Results starting with "Not done" → planned; else Results → in progress; else planned.
pub fn group(e: &Entry) -> Status {
    if let Some(s) = e.status {
        return s;
    }
    let r = e.sections.get("Results").map(|s| s.trim()).unwrap_or("");
    let low = r.to_lowercase();
    if low.starts_with("non réalisé") || low.starts_with("non realise") || low.starts_with("not done") {
        Status::Planned
    } else if !r.is_empty() {
        Status::InProgress
    } else {
        Status::Planned
    }
}

/// Days cited in an entry (DD/MM[/YYYY], YYYY-MM-DD), up to today, ignoring snapshots
/// ("au 03/11", "Figures du 03/11", "as of 2026-11-03"). DD/MM without year: this year, or the
/// previous one if that would be after today.
pub fn cited_days(text: &str, today: Date) -> Vec<Date> {
    let snapshot = Regex::new(r"(?i)(\bau|\bfigures?\s+(?:du|of)|\bas\s+of)\s+[\d/\-]+").unwrap();
    let text = snapshot.replace_all(text, " ");
    let token = Regex::new(r"[\d/\-]+").unwrap();
    let dmy = Regex::new(r"^(\d{1,2})/(\d{1,2})(?:/(\d{4}))?$").unwrap();
    let ymd = Regex::new(r"^(\d{4})-(\d{2})-(\d{2})$").unwrap();
    let mut out = Vec::new();
    for t in token.find_iter(&text) {
        let t = t.as_str();
        let d = if let Some(m) = ymd.captures(t) {
            Date::new(m[1].parse().unwrap_or(0), m[2].parse().unwrap_or(0), m[3].parse().unwrap_or(0)).ok()
        } else if let Some(m) = dmy.captures(t) {
            let (day, mon) = (m[1].parse().unwrap_or(0), m[2].parse().unwrap_or(0));
            match m.get(3) {
                Some(y) => Date::new(y.as_str().parse().unwrap_or(0), mon, day).ok(),
                None => Date::new(today.year(), mon, day)
                    .ok()
                    .map(|d| if d > today { d.checked_sub(1.year()).unwrap_or(d) } else { d }),
            }
        } else {
            None
        };
        if let Some(d) = d.filter(|d| *d <= today) {
            out.push(d);
        }
    }
    out.sort();
    out.dedup();
    out
}

/// First and last file modification of each local day (files at 00:00:00 or 23:59:59 local excluded:
/// dates without a time, e.g. extracted archives).
pub fn day_times(stamps: impl IntoIterator<Item = Timestamp>, tz: &TimeZone) -> BTreeMap<Date, (Timestamp, Timestamp)> {
    let mut m: BTreeMap<Date, (Timestamp, Timestamp)> = BTreeMap::new();
    for t in stamps {
        let z = t.to_zoned(tz.clone());
        let hms = (z.hour(), z.minute(), z.second());
        if hms == (0, 0, 0) || hms == (23, 59, 59) {
            continue;
        }
        let e = m.entry(z.date()).or_insert((t, t));
        e.0 = e.0.min(t);
        e.1 = e.1.max(t);
    }
    m
}

/// Modification times of the files under the data directories.
pub fn scan(dirs: &[std::path::PathBuf]) -> Vec<Timestamp> {
    dirs.iter()
        .flat_map(|d| walkdir::WalkDir::new(d).into_iter().filter_map(|e| e.ok()))
        .filter(|e| e.file_type().is_file())
        .filter_map(|e| e.metadata().ok()?.modified().ok())
        .filter_map(|t| Timestamp::try_from(t).ok())
        .collect()
}

#[derive(Debug, PartialEq)]
pub struct Span {
    pub start_day: Date,
    pub end_day: Date,
    /// ISO UTC instants sent to Labguru.
    pub start: String,
    pub end: String,
    pub duration: i64,
}

/// Dates of an entry: header, else the cited days; times from the data files of those days.
pub fn span(e: &Entry, today: Date, times: &BTreeMap<Date, (Timestamp, Timestamp)>, tz: &TimeZone) -> Option<Span> {
    let cited = cited_days(&e.text, today);
    let start_day = e.start.or(cited.first().copied())?;
    let end_day = e.end.or(cited.last().copied()).unwrap_or(start_day).max(start_day);
    let midnight = |d: Date| d.to_zoned(tz.clone()).map(|z| z.timestamp()).unwrap_or_default();
    let start = times.get(&start_day).map(|t| t.0).unwrap_or_else(|| midnight(start_day));
    let end = times.get(&end_day).map(|t| t.1).unwrap_or_else(|| midnight(end_day)).max(start);
    Some(Span {
        start_day,
        end_day,
        start: start.to_string(),
        end: end.to_string(),
        duration: crate::labguru::days(start_day, end_day),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use jiff::civil::date;

    #[test]
    fn header_and_sections() {
        let t = "---\nstatut: en cours\ndébut: 19/10/2026\nfin: 2026-10-21\n---\n# Run 1\n\n## Description\nd\n## Procédure\n```\n## not a title\n```\n## Résultats\nr\n## Other\nx";
        let e = parse_entry(t, "f").unwrap();
        assert_eq!(e.title, "Run 1");
        assert_eq!(e.status, Some(Status::InProgress));
        assert_eq!((e.start, e.end), (Some(date(2026, 10, 19)), Some(date(2026, 10, 21))));
        assert_eq!(e.sections.keys().collect::<Vec<_>>(), ["Description", "Procedure", "Results"]);
        assert_eq!(e.sections["Procedure"], "```\n## not a title\n```");
        assert!(parse_entry("---\nstatus: maybe\n---\n# x", "f").is_err());
        assert!(parse_entry("---\nstart: 2026-10-21\nend: 2026-10-20\n---\n", "f").is_err());
        assert_eq!(parse_entry("no title", "file").unwrap().title, "file");
        for (s, want) in [
            ("terminée", Status::Done),
            ("done", Status::Done),
            ("Terminées", Status::Done),
            ("in progress", Status::InProgress),
            ("prévue", Status::Planned),
            ("planned", Status::Planned),
        ] {
            assert_eq!(Status::parse(s), Some(want), "{s}");
        }
    }

    #[test]
    fn groups() {
        let g = |r: &str| group(&parse_entry(&format!("# t\n## Results\n{r}"), "f").unwrap());
        assert_eq!(g("19/10: done"), Status::InProgress);
        assert_eq!(g("Non réalisé au 03/11."), Status::Planned);
        assert_eq!(g("Not done yet"), Status::Planned);
        assert_eq!(g(""), Status::Planned);
        assert_eq!(group(&parse_entry("---\nstatus: done\n---\n# t", "f").unwrap()), Status::Done);
    }

    #[test]
    fn dates() {
        let today = date(2026, 11, 3);
        let t = "plan du 28/08/2026, passages du 19/10 et 2026-10-29, ratio 500 / 2000, plan 4/6/5, embargo 31/01/2028, 31/02/2026";
        assert_eq!(cited_days(t, today), [date(2026, 8, 28), date(2026, 10, 19), date(2026, 10, 29)]);
        let snap = "Le 19/10/2026, fait. Bloc en cours au 03/11/2026. Figures du 02/11/2026. Status as of 2026-11-01.";
        assert_eq!(cited_days(snap, today), [date(2026, 10, 19)]);
        // 15/12 without a year in early January → last year
        assert_eq!(cited_days("le 15/12", date(2027, 1, 5)), [date(2026, 12, 15)]);
    }

    #[test]
    fn times_and_timezones() {
        let paris = TimeZone::fixed(jiff::tz::offset(2));
        let ts = |s: &str| s.parse::<Timestamp>().unwrap();
        let stamps = [
            ts("2026-10-18T23:30:00Z"), // 01:30 on the 19th in Paris
            ts("2026-10-19T09:41:19Z"),
            ts("2026-10-19T21:59:59Z"), // 23:59:59 in Paris: excluded
            ts("2026-10-20T22:00:00Z"), // 00:00:00 in Paris: excluded
        ];
        let m = day_times(stamps, &paris);
        assert_eq!(m[&date(2026, 10, 19)], (stamps[0], stamps[1]));
        assert_eq!(m.len(), 1);
        let utc = day_times(stamps, &TimeZone::UTC);
        assert_eq!(utc[&date(2026, 10, 18)].0, stamps[0]);

        let e = parse_entry("# t\n## Results\nle 19/10/2026 puis le 21/10/2026", "f").unwrap();
        let s = span(&e, date(2026, 11, 3), &m, &paris).unwrap();
        assert_eq!(s.start, "2026-10-18T23:30:00Z");
        assert_eq!(s.end, "2026-10-20T22:00:00Z"); // midnight of the 21st, no file that day
        assert_eq!(s.duration, 3);
        let h = parse_entry("---\nstart: 2026-10-19\n---\n# t", "f").unwrap();
        let s = span(&h, date(2026, 11, 3), &m, &paris).unwrap();
        assert_eq!(
            (s.start.as_str(), s.end.as_str(), s.duration),
            ("2026-10-18T23:30:00Z", "2026-10-19T09:41:19Z", 1)
        );
        assert!(span(&parse_entry("# t", "f").unwrap(), date(2026, 11, 3), &m, &paris).is_none());
    }
}
