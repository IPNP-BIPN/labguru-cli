//! Output like gh: a table on a terminal, TSV otherwise, or JSON (--json, --jq, --template).

use anyhow::{Result, anyhow, bail};
use clap::Args;
use comfy_table::{Attribute, Cell, ContentArrangement, Table, presets};
use serde_json::{Map, Value};
use std::io::{IsTerminal, Write};

#[derive(Args, Debug, Default, Clone)]
pub struct OutArgs {
    /// Output JSON with the given fields (comma-separated); without fields, list them
    #[arg(long, value_name = "FIELDS", num_args = 0..=1, default_missing_value = "")]
    pub json: Option<String>,
    /// Filter JSON output with a jq expression
    #[arg(long, short = 'q', value_name = "EXPR")]
    pub jq: Option<String>,
    /// Format JSON output with a template: {{.field}}
    #[arg(long, short = 't', value_name = "TEMPLATE")]
    pub template: Option<String>,
}

impl OutArgs {
    pub fn is_json(&self) -> bool {
        self.json.is_some() || self.jq.is_some() || self.template.is_some()
    }
}

pub fn stdout_tty() -> bool {
    std::io::stdout().is_terminal()
}

pub fn color() -> bool {
    std::env::var_os("NO_COLOR").is_none_or(|v| v.is_empty()) && stdout_tty()
}

/// Interactive prompts only on a terminal, and unless disabled in the settings.
pub fn can_prompt() -> bool {
    std::io::stdin().is_terminal()
        && std::io::stderr().is_terminal()
        && crate::config::Config::load()
            .map(|c| c.setting("prompt") != "disabled")
            .unwrap_or(true)
}

/// `a.b` in an object; `x_id` also finds `x.id` (Labguru returns either).
pub fn field<'a>(v: &'a Value, path: &str) -> Option<&'a Value> {
    let mut cur = v;
    for part in path.split('.') {
        cur = match cur.get(part) {
            Some(x) if !x.is_null() => x,
            _ => part.strip_suffix("_id").and_then(|p| cur.get(p)).and_then(|o| o.get("id"))?,
        };
    }
    Some(cur)
}

pub fn cell(v: Option<&Value>) -> String {
    match v {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(s)) => s.clone(),
        Some(Value::Bool(b)) => b.to_string(),
        Some(Value::Number(n)) => n.to_string(),
        Some(x) => x.to_string(),
    }
}

/// Keeps only the requested fields (error listing the available ones otherwise).
pub fn select(rows: &[Value], fields: &str) -> Result<Vec<Value>> {
    let mut avail: Vec<String> = rows.iter().filter_map(|r| r.as_object()).flat_map(|o| o.keys().cloned()).collect();
    avail.sort();
    avail.dedup();
    let wanted: Vec<&str> = fields.split(',').map(str::trim).filter(|s| !s.is_empty()).collect();
    if wanted.is_empty() {
        bail!(
            tr!(
                "Specify one or more comma-separated fields for --json:\n  {}",
                "Indiquez un ou plusieurs champs, séparés par des virgules, pour --json :\n  {}"
            )
            .replace("{}", &avail.join("\n  "))
        );
    }
    if let Some(bad) = wanted
        .iter()
        .find(|w| !rows.is_empty() && rows.iter().all(|r| field(r, w).is_none()) && !avail.contains(&w.to_string()))
    {
        bail!(
            tr!(
                "Unknown JSON field: {bad}. Available fields:\n  {}",
                "Champ JSON inconnu : {bad}. Champs disponibles :\n  {}"
            )
            .replace("{bad}", bad)
            .replace("{}", &avail.join("\n  "))
        );
    }
    Ok(rows
        .iter()
        .map(|r| {
            Value::Object(
                wanted
                    .iter()
                    .map(|w| (w.to_string(), field(r, w).cloned().unwrap_or(Value::Null)))
                    .collect::<Map<_, _>>(),
            )
        })
        .collect())
}

/// Runs a jq filter (jaq) and returns each output, strings unquoted, like gh --jq.
pub fn jq(v: &Value, expr: &str) -> Result<Vec<String>> {
    use jaq_core::load::{Arena, File, Loader};
    use jaq_core::{Compiler, Ctx, Vars, data, unwrap_valr};
    use jaq_json::Val;
    let input = jaq_json::read::parse_single(serde_json::to_string(v)?.as_bytes()).map_err(|e| anyhow!("{e}"))?;
    let defs = jaq_core::defs().chain(jaq_std::defs()).chain(jaq_json::defs());
    let funs = jaq_core::funs().chain(jaq_std::funs()).chain(jaq_json::funs());
    let arena = Arena::default();
    let modules = Loader::new(defs)
        .load(&arena, File { code: expr, path: () })
        .map_err(|_| anyhow!(tr!("invalid jq expression: {expr}", "expression jq invalide : {expr}").replace("{expr}", expr)))?;
    let filter = Compiler::default()
        .with_funs(funs)
        .compile(modules)
        .map_err(|_| anyhow!(tr!("invalid jq expression: {expr}", "expression jq invalide : {expr}").replace("{expr}", expr)))?;
    let ctx = Ctx::<data::JustLut<Val>>::new(&filter.lut, Vars::new([]));
    let mut out = Vec::new();
    for r in filter.id.run((ctx, input)).map(unwrap_valr) {
        let v = r.map_err(|e| anyhow!("jq: {e}"))?;
        out.push(match &v {
            Val::TStr(_) => {
                let s = v.to_string();
                serde_json::from_str::<String>(&s).unwrap_or(s)
            }
            _ => v.to_string(),
        });
    }
    Ok(out)
}

/// Minimal template: {{.field}} or {{.a.b}} per item, {{.}} for the whole value; \n and \t escapes.
pub fn template(v: &Value, t: &str) -> String {
    let re = regex::Regex::new(r"\{\{\s*\.?([\w.]*)\s*\}\}").unwrap();
    let t = t.replace("\\n", "\n").replace("\\t", "\t");
    let one = |x: &Value| {
        re.replace_all(
            &t,
            |c: &regex::Captures| if c[1].is_empty() { cell(Some(x)) } else { cell(field(x, &c[1])) },
        )
        .to_string()
    };
    match v {
        Value::Array(a) => a.iter().map(one).collect(),
        x => one(x),
    }
}

/// Prints a JSON value according to --json/--jq/--template (fields already selected).
pub fn emit_json(w: &mut dyn Write, v: &Value, out: &OutArgs) -> Result<()> {
    if let Some(e) = &out.jq {
        for line in jq(v, e)? {
            writeln!(w, "{line}")?;
        }
    } else if let Some(t) = &out.template {
        write!(w, "{}", template(v, t))?;
    } else {
        writeln!(w, "{}", serde_json::to_string_pretty(v)?)?;
    }
    Ok(())
}

pub fn render_table(w: &mut dyn Write, rows: &[Value], cols: &[(&str, &str)], tty: bool, color: bool) -> Result<()> {
    if !tty {
        for r in rows {
            let line: Vec<String> = cols
                .iter()
                .map(|(_, f)| cell(field(r, f)).replace(['\t', '\n', '\r'], " "))
                .collect();
            writeln!(w, "{}", line.join("\t"))?;
        }
        return Ok(());
    }
    let mut t = Table::new();
    t.load_style(presets::NOTHING).set_content_arrangement(ContentArrangement::Dynamic);
    t.set_header(cols.iter().map(|(h, _)| {
        if color {
            Cell::new(h).add_attribute(Attribute::Bold)
        } else {
            Cell::new(h)
        }
    }));
    for r in rows {
        t.add_row(cols.iter().map(|(_, f)| cell(field(r, f))));
    }
    writeln!(w, "{}", t.lines().map(|l| l.trim_end().to_string()).collect::<Vec<_>>().join("\n"))?;
    Ok(())
}

/// A list: JSON if asked, else table/TSV.
pub fn print_list(rows: &[Value], cols: &[(&str, &str)], out: &OutArgs) -> Result<()> {
    let mut w = std::io::stdout().lock();
    if out.is_json() {
        let v = match &out.json {
            Some(f) => Value::Array(select(rows, f)?),
            None => Value::Array(rows.to_vec()),
        };
        return emit_json(&mut w, &v, out);
    }
    if rows.is_empty() && stdout_tty() {
        eprintln!("{}", tr!("No results.", "Aucun résultat."));
        return Ok(());
    }
    render_table(&mut w, rows, cols, stdout_tty(), color())
}

pub fn render_object(w: &mut dyn Write, v: &Value, fields: &[(&str, &str)], tty: bool, color: bool) -> Result<()> {
    for (label, f) in fields {
        let c = cell(field(v, f));
        if c.is_empty() {
            continue;
        }
        if tty {
            let label = if color {
                format!("\x1b[1m{label}:\x1b[0m")
            } else {
                format!("{label}:")
            };
            writeln!(w, "{label} {c}")?;
        } else {
            writeln!(w, "{label}\t{}", c.replace(['\t', '\n', '\r'], " "))?;
        }
    }
    Ok(())
}

pub fn print_object(v: &Value, fields: &[(&str, &str)], out: &OutArgs) -> Result<()> {
    let mut w = std::io::stdout().lock();
    if out.is_json() {
        let v = match &out.json {
            Some(f) => select(std::slice::from_ref(v), f)?.remove(0),
            None => v.clone(),
        };
        return emit_json(&mut w, &v, out);
    }
    render_object(&mut w, v, fields, stdout_tty(), color())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn rows() -> Vec<Value> {
        vec![
            json!({"id": 1, "title": "A\tb", "project": {"id": 7}}),
            json!({"id": 2, "title": "C", "project_id": 8}),
        ]
    }

    #[test]
    fn tsv_and_table() {
        let cols = [("ID", "id"), ("TITLE", "title"), ("PROJECT", "project_id")];
        let mut w = Vec::new();
        render_table(&mut w, &rows(), &cols, false, false).unwrap();
        assert_eq!(String::from_utf8(w).unwrap(), "1\tA b\t7\n2\tC\t8\n");
        let mut w = Vec::new();
        render_table(&mut w, &rows(), &cols, true, false).unwrap();
        let s = String::from_utf8(w).unwrap();
        assert!(
            s.lines().next().unwrap().contains("ID") && s.contains("PROJECT") && !s.contains('\x1b'),
            "{s}"
        );
        let mut w = Vec::new();
        render_object(&mut w, &rows()[1], &[("Title", "title"), ("Empty", "zz")], false, false).unwrap();
        assert_eq!(String::from_utf8(w).unwrap(), "Title\tC\n");
    }

    #[test]
    fn json_fields() {
        assert_eq!(
            select(&rows(), "id,project_id").unwrap(),
            vec![json!({"id": 1, "project_id": 7}), json!({"id": 2, "project_id": 8})]
        );
        assert!(select(&rows(), "").unwrap_err().to_string().contains("title"));
        assert!(select(&rows(), "nope").is_err());
        let mut w = Vec::new();
        emit_json(&mut w, &json!([{"a": 1}]), &OutArgs::default()).unwrap();
        assert_eq!(String::from_utf8(w).unwrap(), "[\n  {\n    \"a\": 1\n  }\n]\n");
    }

    #[test]
    fn jq_and_template() {
        assert_eq!(jq(&Value::Array(rows()), ".[] | .title").unwrap(), ["A\tb", "C"]);
        assert_eq!(jq(&json!({"n": [1, 2]}), ".n | length").unwrap(), ["2"]);
        assert!(jq(&json!(1), ".[").is_err());
        assert_eq!(template(&Value::Array(rows()), "{{.id}}:{{ .project.id }}\\n"), "1:7\n2:\n");
    }
}
