//! Minimal BibTeX reader: @type{key, field = {…} | "…" | number, …} → Labguru paper items.
//! shortcut: no @string macros nor # concatenation; add them if a real .bib needs it.

use regex::Regex;
use serde_json::{Map, Value};
use std::collections::BTreeMap;

pub type Entry = BTreeMap<String, String>;

const ACCENTS: &[(char, &str, &str)] = &[
    ('\'', "aeiouyAEIOUYcnszCNSZ", "áéíóúýÁÉÍÓÚÝćńśźĆŃŚŹ"),
    ('`', "aeiouAEIOU", "àèìòùÀÈÌÒÙ"),
    ('^', "aeiouAEIOUcgsCGS", "âêîôûÂÊÎÔÛĉĝŝĈĜŜ"),
    ('"', "aeiouyAEIOUY", "äëïöüÿÄËÏÖÜŸ"),
    ('~', "anoANO", "ãñõÃÑÕ"),
    ('=', "aeiouAEIOU", "āēīōūĀĒĪŌŪ"),
    ('.', "zZeEcC", "żŻėĖċĊ"),
    ('c', "cCsStT", "çÇşŞţŢ"),
    ('u', "aAgGuU", "ăĂğĞŭŬ"),
    ('v', "cCsSzZrReEnNdD", "čČšŠžŽřŘěĚňŇďĎ"),
    ('H', "oOuU", "őŐűŰ"),
    ('k', "aAeE", "ąĄęĘ"),
    ('r', "aAuU", "åÅůŮ"),
];

fn accent(cmd: char, letter: &str) -> String {
    let l = if letter == "\\i" { "i" } else { letter };
    let c = l.chars().next().unwrap_or(' ');
    ACCENTS
        .iter()
        .find(|(a, _, _)| *a == cmd)
        .and_then(|(_, base, done)| base.chars().position(|b| b == c).and_then(|i| done.chars().nth(i)))
        .map(String::from)
        .unwrap_or_else(|| l.to_string()) // shortcut: an unknown pair drops the accent; extend ACCENTS if it matters
}

/// LaTeX accents → Unicode, braces removed, typographic dashes.
pub fn latex(t: &str) -> String {
    let acc = Regex::new(r#"\\([`'^"~=.]|[cuvHkr])(\s*\{\s*(\\i|[A-Za-z])\s*\}|\s*(\\i|[A-Za-z]))"#).unwrap();
    let t = acc.replace_all(t, |m: &regex::Captures| {
        let cmd = m[1].chars().next().unwrap();
        // \cite, \url…: a letter command glued to more letters is not an accent
        if cmd.is_ascii_alphabetic() && m[2].starts_with(|c: char| c.is_ascii_alphabetic()) {
            return m[0].to_string();
        }
        accent(cmd, m.get(3).or(m.get(4)).map_or("", |x| x.as_str()))
    });
    let special = Regex::new(r"\\(ss|ae|AE|oe|OE|aa|AA|[oOlLi])([A-Za-z]?)\s?").unwrap();
    let t = special.replace_all(&t, |m: &regex::Captures| {
        if !m[2].is_empty() {
            return m[0].to_string();
        }
        match &m[1] {
            "ss" => "ß",
            "o" => "ø",
            "O" => "Ø",
            "ae" => "æ",
            "AE" => "Æ",
            "oe" => "œ",
            "OE" => "Œ",
            "aa" => "å",
            "AA" => "Å",
            "l" => "ł",
            "L" => "Ł",
            _ => "ı",
        }
        .to_string()
    });
    let t = Regex::new(r"\\([&%_$#])").unwrap().replace_all(&t, "$1");
    let t = t.replace("---", "—").replace("--", "–").replace('~', " ").replace(['{', '}'], "");
    t.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn parse(text: &str) -> Vec<Entry> {
    let text = Regex::new(r"(?m)^\s*%.*$").unwrap().replace_all(text, "").to_string();
    let head = Regex::new(r"@(\w+)\s*\{\s*([^,\s{}]*)\s*,").unwrap();
    let field = Regex::new(r"^\s*,?\s*(\w[\w-]*)\s*=\s*").unwrap();
    let bare = Regex::new(r"^[^,}\s]+").unwrap();
    let b = text.as_bytes();
    let (mut out, mut pos) = (Vec::new(), 0);
    while let Some(m) = head.captures_at(&text, pos) {
        let (kind, key) = (m[1].to_lowercase(), m[2].to_string());
        let mut i = m.get(0).unwrap().end();
        let mut e = Entry::new();
        while let Some(f) = field.captures(&text[i..]) {
            let name = f[1].to_lowercase();
            i += f.get(0).unwrap().end();
            let val;
            match b.get(i) {
                Some(&q @ (b'{' | b'"')) => {
                    let (mut depth, mut j) = (0i32, i);
                    while j < b.len() {
                        depth += match b[j] {
                            b'{' => 1,
                            b'}' => -1,
                            _ => 0,
                        };
                        if (q == b'{' && depth == 0) || (q == b'"' && j > i && b[j] == b'"' && depth == 0) {
                            break;
                        }
                        j += 1;
                    }
                    val = &text[i + 1..j.min(b.len())];
                    i = (j + 1).min(b.len());
                }
                _ => match bare.find(&text[i..]) {
                    Some(v) => {
                        val = v.as_str();
                        i += v.end();
                    }
                    None => break,
                },
            }
            e.insert(name, latex(val));
        }
        pos = i.max(m.get(0).unwrap().end());
        if !["comment", "string", "preamble"].contains(&kind.as_str()) {
            e.insert("type".into(), kind);
            e.insert("key".into(), key);
            out.push(e);
        }
    }
    out
}

const MONTHS: [&str; 12] = ["jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec"];

/// BibTeX entry → Labguru paper `item`.
pub fn paper_item(b: &Entry) -> Map<String, Value> {
    let g = |k: &str| b.get(k).cloned().unwrap_or_default();
    let year: String = g("year").chars().take(4).collect();
    let mon = g("month").to_lowercase();
    let month = mon
        .parse::<u32>()
        .ok()
        .or_else(|| MONTHS.iter().position(|m| mon.starts_with(m)).map(|i| i as u32 + 1))
        .unwrap_or(1);
    let doi = g("doi").trim_start_matches("https://doi.org/").to_string();
    let authors = Regex::new(r"\s+and\s+").unwrap().split(&g("author")).collect::<Vec<_>>().join("; ");
    let date = (year.len() == 4 && year.chars().all(|c| c.is_ascii_digit())).then(|| format!("{year}-{month:02}-01"));
    let url = b
        .get("url")
        .cloned()
        .or_else(|| (!doi.is_empty()).then(|| format!("https://doi.org/{doi}")));
    let journal = b.get("journal").or(b.get("booktitle")).or(b.get("publisher")).cloned();
    let review = b.get("abstract").or(b.get("note")).cloned();
    let pairs = [
        ("name", b.get("title").cloned()),
        ("authors", Some(authors)),
        ("journal", journal),
        ("publication_date", date),
        ("volume", b.get("volume").cloned()),
        ("number", b.get("number").cloned()),
        ("pages", b.get("pages").cloned()),
        ("doi", Some(doi)),
        ("url", url),
        ("review", review),
    ];
    pairs
        .into_iter()
        .filter_map(|(k, v)| v.filter(|v| !v.is_empty()).map(|v| (k.to_string(), Value::String(v))))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bibtex() {
        let b = parse(concat!(
            "% comment @x{y,\n",
            r#"@article{k1, author = {Guti{\'e}rrez, Emilio and M{\"u}ller, J\"org}, title = {{A} rat --- test \& co},"#,
            "\n year = 1968, month = mar, pages = {414--421}, doi = {10.1/X}, note = \"n {b}\"}\n",
            "@comment{skip}\n",
            r"@book{k2, title={\c{C}a {\^\i}le \ss}, author={\o}, note={\cite{x}}}"
        ));
        assert_eq!(b.iter().map(|x| x["key"].as_str()).collect::<Vec<_>>(), ["k1", "k2"]);
        let p = paper_item(&b[0]);
        let want = serde_json::json!({"name": "A rat — test & co", "authors": "Gutiérrez, Emilio; Müller, Jörg",
            "pages": "414–421", "publication_date": "1968-03-01", "doi": "10.1/X", "url": "https://doi.org/10.1/X", "review": "n b"});
        assert_eq!(Value::Object(p), want);
        assert_eq!(b[1]["title"], "Ça île ß");
        assert_eq!(b[1]["author"], "ø");
        assert_eq!(b[1]["note"], r"\citex");
    }
}
