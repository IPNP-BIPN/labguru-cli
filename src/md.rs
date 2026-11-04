//! Markdown → HTML for Labguru: tables, inline <code> as a monospace span (Labguru shows <code> as a
//! block), headings shifted under the section title (h1/h2 → h3), no anchors, local images rewritten.

use pulldown_cmark::{CowStr, Event, HeadingLevel, Options, Parser, Tag, TagEnd};

pub fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn shift(l: HeadingLevel) -> HeadingLevel {
    HeadingLevel::try_from((l as usize + 1).clamp(3, 6)).unwrap_or(HeadingLevel::H6)
}

/// True for a relative path to a local file (not a URL, not an absolute site path, not an anchor).
pub fn is_local(src: &str) -> bool {
    !(src.contains("://") || src.starts_with('/') || src.starts_with('#') || src.starts_with("data:") || src.is_empty())
}

/// `image(src)` returns the URL to use for a local image, or None to keep it unchanged.
pub fn to_html(md: &str, image: &mut dyn FnMut(&str) -> Option<String>) -> String {
    let opts = Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS;
    let events = Parser::new_ext(md, opts).map(|e| match e {
        Event::Code(c) => Event::InlineHtml(CowStr::from(format!(
            "<span style=\"font-family: monospace\">{}</span>",
            escape(&c)
        ))),
        Event::Start(Tag::Heading { level, .. }) => Event::Start(Tag::Heading {
            level: shift(level),
            id: None,
            classes: vec![],
            attrs: vec![],
        }),
        Event::End(TagEnd::Heading(l)) => Event::End(TagEnd::Heading(shift(l))),
        Event::Start(Tag::Image {
            link_type,
            dest_url,
            title,
            id,
        }) => {
            let dest_url = match is_local(&dest_url).then(|| image(&dest_url)).flatten() {
                Some(u) => CowStr::from(u),
                None => dest_url,
            };
            Event::Start(Tag::Image {
                link_type,
                dest_url,
                title,
                id,
            })
        }
        e => e,
    });
    let mut out = String::new();
    pulldown_cmark::html::push_html(&mut out, events);
    out
}

/// Local image paths referenced by a Markdown text.
pub fn local_images(md: &str) -> Vec<String> {
    let mut out = Vec::new();
    for e in Parser::new_ext(md, Options::ENABLE_TABLES) {
        if let Event::Start(Tag::Image { dest_url, .. }) = e
            && is_local(&dest_url)
            && !out.contains(&dest_url.to_string())
        {
            out.push(dest_url.to_string());
        }
    }
    out
}

/// HTML → plain text, for terminal display.
pub fn html_to_text(h: &str) -> String {
    let br = regex::Regex::new(r"(?i)<br\s*/?>|</(p|div|h[1-6]|li|tr)>").unwrap();
    let tag = regex::Regex::new(r"<[^>]*>").unwrap();
    let lines = br.replace_all(h, "\n");
    let t = tag.replace_all(&lines, "");
    let t = t
        .replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&amp;", "&");
    let blank = regex::Regex::new(r"\n\s*\n+").unwrap();
    blank.replace_all(t.trim(), "\n\n").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render() {
        let mut none = |_: &str| None;
        let h = to_html("# T\n\n## Sub\n\ntext `x<1` and\n\n```\nblock\n```\n\n#### deep\n", &mut none);
        assert_eq!(
            h,
            "<h3>T</h3>\n<h3>Sub</h3>\n<p>text <span style=\"font-family: monospace\">x&lt;1</span> and</p>\n<pre><code>block\n</code></pre>\n<h5>deep</h5>\n"
        );
        let t = to_html("| a | b |\n|---|---|\n| 1 | 2 |\n", &mut none);
        assert!(t.contains("<table>") && t.contains("<td>1</td>"), "{t}");
        assert!(!to_html("## H {#anchor}", &mut none).contains("id="));
    }

    #[test]
    fn images() {
        let md = "![a](fig/a.png) ![b](https://x.org/b.png) ![c](/abs.png)";
        assert_eq!(local_images(md), ["fig/a.png"]);
        let h = to_html(md, &mut |s| Some(format!("/user_assets/1/original/{s}")));
        assert!(
            h.contains("src=\"/user_assets/1/original/fig/a.png\"") && h.contains("src=\"https://x.org/b.png\""),
            "{h}"
        );
        assert!(!h.contains("width"));
    }

    #[test]
    fn text() {
        assert_eq!(html_to_text("<h3>T</h3><p>a &amp; b<br>c</p>"), "T\na & b\nc");
    }
}
