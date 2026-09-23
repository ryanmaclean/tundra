//! Safe rendering of untrusted Markdown (e.g. GitHub issue bodies) to HTML.
//!
//! Bead/task descriptions can come from external sources (GitHub issue sync),
//! so they are attacker-controlled. The output of this module is the ONLY
//! user-derived string that may be passed to `inner_html`.
//!
//! Guarantees:
//! - Raw HTML blocks and inline HTML in the source are rendered as escaped
//!   text, never as markup (no `<script>`, `<img onerror=...>`, `<iframe>`).
//! - Link and image destinations are restricted to an allowlist of schemes
//!   (`http`, `https`, `mailto`) or scheme-less relative/fragment URLs;
//!   anything else (`javascript:`, `data:`, `vbscript:`, ...) becomes `#`.
//! - Everything else is emitted by pulldown-cmark, which HTML-escapes text
//!   and attribute values.
//!
//! `ammonia` is not usable in WASM, so this is an allowlist-by-construction
//! approach instead of a post-hoc HTML sanitizer.

/// HTML-escape `s` for use as element text or a quoted attribute value.
pub fn escape_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

/// Schemes permitted in link/image destinations.
const ALLOWED_SCHEMES: &[&str] = &["http", "https", "mailto"];

/// Returns true when `url` is safe to place in an `href`/`src`: either it has
/// no scheme (relative path, `#fragment`, `?query`) or its scheme is in
/// [`ALLOWED_SCHEMES`].
///
/// Browsers strip ASCII whitespace/control characters when parsing a scheme
/// (`java\tscript:` is `javascript:`), so those are removed before checking.
pub fn is_safe_url(url: &str) -> bool {
    let normalized: String = url
        .chars()
        .filter(|c| !c.is_ascii_whitespace() && !c.is_control())
        .collect::<String>()
        .to_ascii_lowercase();

    // Find the scheme delimiter before any path/query/fragment character.
    match normalized.find([':', '/', '?', '#']) {
        Some(i) if normalized.as_bytes()[i] == b':' => {
            let scheme = &normalized[..i];
            ALLOWED_SCHEMES.contains(&scheme)
        }
        // No scheme: relative URL, fragment, or query.
        _ => true,
    }
}

/// Render untrusted Markdown into HTML that is safe for `inner_html`.
#[cfg(feature = "markdown")]
pub fn render_markdown_safe(src: &str) -> String {
    use pulldown_cmark::{html, CowStr, Event, Parser, Tag};

    fn safe_dest(url: CowStr<'_>) -> CowStr<'_> {
        if is_safe_url(&url) {
            url
        } else {
            CowStr::Borrowed("#")
        }
    }

    let parser = Parser::new(src).map(|event| match event {
        // Raw HTML is displayed as text (push_html escapes Text events).
        Event::Html(raw) | Event::InlineHtml(raw) => Event::Text(raw),
        Event::Start(Tag::Link {
            link_type,
            dest_url,
            title,
            id,
        }) => Event::Start(Tag::Link {
            link_type,
            dest_url: safe_dest(dest_url),
            title,
            id,
        }),
        Event::Start(Tag::Image {
            link_type,
            dest_url,
            title,
            id,
        }) => Event::Start(Tag::Image {
            link_type,
            dest_url: safe_dest(dest_url),
            title,
            id,
        }),
        other => other,
    });

    let mut out = String::new();
    html::push_html(&mut out, parser);
    out
}

/// Without the `markdown` feature: escape everything and keep line breaks.
#[cfg(not(feature = "markdown"))]
pub fn render_markdown_safe(src: &str) -> String {
    escape_html(src).replace('\n', "<br>")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Tags pulldown-cmark may legitimately emit (CommonMark, no extensions).
    const ALLOWED_TAGS: &[&str] = &[
        "p", "br", "hr", "h1", "h2", "h3", "h4", "h5", "h6", "em", "strong", "code", "pre",
        "blockquote", "ul", "ol", "li", "a", "img",
    ];
    const ALLOWED_ATTRS: &[&str] = &["href", "src", "alt", "title", "class", "start"];

    /// Scan every tag in `html`: only allowlisted tags/attributes, and every
    /// href/src value must pass [`is_safe_url`]. Text content is ignored, so
    /// escaped payloads (`&lt;img onerror=...&gt;`) are correctly treated as
    /// inert.
    fn assert_inert(html: &str) {
        let mut rest = html;
        while let Some(open) = rest.find('<') {
            let after = &rest[open + 1..];
            let close = after.find('>').unwrap_or_else(|| panic!("unclosed tag in {html}"));
            let tag = &after[..close];
            rest = &after[close + 1..];

            let body = tag.trim_start_matches('/').trim_end_matches('/');
            let name = body.split_whitespace().next().unwrap_or("");
            assert!(ALLOWED_TAGS.contains(&name), "tag {name:?} in {html}");

            // Walk attributes: name="value" pairs (pulldown-cmark always quotes).
            let mut attrs = body[name.len()..].trim();
            while !attrs.is_empty() {
                let eq = attrs.find('=').unwrap_or_else(|| panic!("bare attr in {tag:?}"));
                let attr = attrs[..eq].trim();
                assert!(ALLOWED_ATTRS.contains(&attr), "attr {attr:?} in {html}");
                let v = &attrs[eq + 1..];
                assert!(v.starts_with('"'), "unquoted attr in {tag:?}");
                let vend = v[1..].find('"').unwrap_or_else(|| panic!("unterminated in {tag:?}"));
                let value = &v[1..1 + vend];
                if attr == "href" || attr == "src" {
                    let decoded = value.replace("&amp;", "&").replace("&#58;", ":");
                    assert!(is_safe_url(&decoded), "unsafe {attr}={value:?} in {html}");
                }
                attrs = v[vend + 2..].trim();
            }
        }
    }

    #[test]
    fn escape_html_escapes_all_specials() {
        assert_eq!(
            escape_html(r#"<a href="x" onclick='y'>&</a>"#),
            "&lt;a href=&quot;x&quot; onclick=&#39;y&#39;&gt;&amp;&lt;/a&gt;"
        );
    }

    #[test]
    fn url_allowlist() {
        for ok in [
            "https://github.com/o/r",
            "http://example.com",
            "mailto:a@b.c",
            "/relative/path",
            "relative",
            "#frag",
            "?q=1",
            "path/with:colon",
        ] {
            assert!(is_safe_url(ok), "{ok}");
        }
        for bad in [
            "javascript:alert(1)",
            "JavaScript:alert(1)",
            " javascript:alert(1)",
            "java\tscript:alert(1)",
            "java\nscript:alert(1)",
            "\u{1}javascript:alert(1)",
            "data:text/html,<script>alert(1)</script>",
            "vbscript:msgbox",
            "file:///etc/passwd",
        ] {
            assert!(!is_safe_url(bad), "{bad:?}");
        }
    }

    #[cfg(feature = "markdown")]
    #[test]
    fn raw_html_is_escaped() {
        let payloads = [
            r#"<img src=x onerror="alert(document.domain)">"#,
            "<script>alert(1)</script>",
            "hello <img src=x onerror=alert(1)> inline",
            "<iframe src=\"https://evil\"></iframe>",
            "<svg onload=alert(1)>",
            "<details open ontoggle=alert(1)>",
            "<a href=\"javascript:alert(1)\">x</a>",
        ];
        for p in payloads {
            let out = render_markdown_safe(p);
            assert_inert(&out);
            assert!(out.contains("&lt;"), "raw HTML not escaped: {out}");
        }
    }

    #[cfg(feature = "markdown")]
    #[test]
    fn dangerous_link_schemes_are_neutralised() {
        for src in [
            "[click](javascript:alert(1))",
            "[click](JAVASCRIPT:alert(1))",
            "[click](javascript&#58;alert(1))",
            "[click](<java\tscript:alert(1)>)",
            "[click](data:text/html;base64,PHNjcmlwdD4=)",
            "![img](javascript:alert(1))",
            "[ref]\n\n[ref]: javascript:alert(1)",
            "<javascript:alert(1)>",
        ] {
            let out = render_markdown_safe(src);
            assert_inert(&out);
            let lower = out.to_ascii_lowercase();
            assert!(!lower.contains("href=\"javascript"), "{src} -> {out}");
            assert!(!lower.contains("src=\"javascript"), "{src} -> {out}");
            assert!(!lower.contains("href=\"data:"), "{src} -> {out}");
        }
    }

    #[cfg(feature = "markdown")]
    #[test]
    fn benign_markdown_still_renders() {
        let out = render_markdown_safe(
            "# Title\n\nSome **bold** and `code` with [a link](https://github.com/o/r/issues/1).\n\n- item",
        );
        assert!(out.contains("<h1>Title</h1>"));
        assert!(out.contains("<strong>bold</strong>"));
        assert!(out.contains("<code>code</code>"));
        assert!(out.contains(r#"<a href="https://github.com/o/r/issues/1">a link</a>"#));
        assert!(out.contains("<li>item</li>"));
    }

    #[cfg(feature = "markdown")]
    #[test]
    fn attribute_breakout_is_escaped() {
        for src in [
            r#"[x](https://a.b/"onmouseover="alert(1))"#,
            r#"[x](https://a.b "t\" onmouseover=\"alert(1)")"#,
            r#"![x" onerror="alert(1)](https://a.b/i.png)"#,
        ] {
            assert_inert(&render_markdown_safe(src));
        }
    }

    #[cfg(feature = "markdown")]
    #[test]
    fn inert_checker_rejects_live_payload() {
        // Sanity check the oracle itself.
        let r = std::panic::catch_unwind(|| assert_inert(r#"<img src=x onerror="alert(1)">"#));
        assert!(r.is_err());
    }
}
