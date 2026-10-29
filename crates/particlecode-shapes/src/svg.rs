//! A tiny scanner that pulls the first `<path d="...">` and the `viewBox` out of an `.svg`
//! file. It is not an XML parser: no entities, no CDATA, no `transform`s, no `<use>`.

use crate::error::Error;
use crate::path::ViewBox;

/// What the shape tool reads from an SVG file.
#[derive(Debug, Clone, PartialEq)]
pub struct SvgPath {
    /// The `d` attribute of the first `<path>`.
    pub d: String,
    /// The root `viewBox`, or `0 0 1000 1000` if absent.
    pub view_box: ViewBox,
}

/// Extract the first path of `svg`.
pub fn extract_first_path(svg: &str) -> Result<SvgPath, Error> {
    let mut view_box = None;
    let mut rest = svg;
    while let Some(lt) = rest.find('<') {
        rest = &rest[lt..];
        if let Some(after) = rest.strip_prefix("<!--") {
            rest = after.find("-->").map_or("", |i| &after[i + 3..]);
            continue;
        }
        let (tag, attrs, consumed) = read_tag(rest);
        rest = &rest[consumed..];
        match tag {
            "svg" if view_box.is_none() => {
                if let Some(v) = attribute(attrs, "viewBox") {
                    view_box = Some(ViewBox::parse(&v)?);
                }
            }
            "path" => {
                if attribute(attrs, "transform").is_some() {
                    return Err(Error::BadSvg(
                        "the <path> has a transform attribute, which is not supported; \
                         bake it into the path data"
                            .into(),
                    ));
                }
                let d = attribute(attrs, "d")
                    .ok_or_else(|| Error::BadSvg("the first <path> has no d attribute".into()))?;
                return Ok(SvgPath {
                    d,
                    view_box: view_box.unwrap_or(ViewBox::DESIGN),
                });
            }
            _ => {}
        }
    }
    Err(Error::BadSvg("no <path> element found".into()))
}

/// Split a tag starting at `<` into (name, attribute text, bytes consumed). The tag ends at
/// the first `>` outside quotes.
fn read_tag(s: &str) -> (&str, &str, usize) {
    let bytes = s.as_bytes();
    let mut quote = None;
    let mut end = bytes.len();
    for (i, &b) in bytes.iter().enumerate().skip(1) {
        match (quote, b) {
            (None, b'"' | b'\'') => quote = Some(b),
            (Some(q), b) if b == q => quote = None,
            (None, b'>') => {
                end = i;
                break;
            }
            _ => {}
        }
    }
    let inner = &s[1..end];
    let name_len = inner
        .find(|c: char| c.is_ascii_whitespace() || c == '/')
        .unwrap_or(inner.len());
    let consumed = (end + 1).min(s.len());
    (&inner[..name_len], &inner[name_len..], consumed)
}

/// Value of attribute `name` in the attribute text of a tag.
fn attribute(attrs: &str, name: &str) -> Option<String> {
    let mut rest = attrs;
    loop {
        rest = rest.trim_start_matches(|c: char| c.is_ascii_whitespace() || c == '/');
        let eq = rest.find('=')?;
        let key = rest[..eq].trim();
        let after = rest[eq + 1..].trim_start();
        let quote = after.chars().next().filter(|c| *c == '"' || *c == '\'')?;
        let body = &after[1..];
        let close = body.find(quote)?;
        if key == name {
            return Some(body[..close].to_string());
        }
        rest = &body[close + 1..];
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp)] // reason: tests compare exact, hand-written coordinates
mod tests {
    use super::*;

    #[test]
    fn finds_path_and_view_box() {
        let svg = r#"<?xml version="1.0"?>
            <!-- <path d="M 0 0"/> in a comment -->
            <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">
              <rect width="5" height="5"/>
              <path fill="red" d='M 10 10 L 90 10 L 90 90 Z' />
              <path d="M 1 1"/>
            </svg>"#;
        let p = extract_first_path(svg).unwrap();
        assert_eq!(p.d, "M 10 10 L 90 10 L 90 90 Z");
        assert_eq!(p.view_box.width, 100.0);
    }

    #[test]
    fn defaults_to_design_box_and_rejects_transform() {
        let p = extract_first_path(r#"<svg><path d="M0 0 L 5 5"/></svg>"#).unwrap();
        assert_eq!(p.view_box, ViewBox::DESIGN);
        assert!(extract_first_path(r#"<svg><path transform="scale(2)" d="M0 0"/></svg>"#).is_err());
        assert!(extract_first_path("<svg></svg>").is_err());
        assert!(extract_first_path(r#"<svg><path id="x"/></svg>"#).is_err());
    }

    #[test]
    fn greater_than_inside_quotes_is_fine() {
        let p = extract_first_path(r#"<svg><path data-x="a>b" d="M 1 1 L 2 2"/></svg>"#).unwrap();
        assert_eq!(p.d, "M 1 1 L 2 2");
    }
}
