//! Inert, bounded metadata retained on editable SVG leaf objects.
//! Metadata never enters the paint/resource resolver. Geometry recipes, event
//! handlers, external links and foreign elements are outside this contract.
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const INKSCAPE: &str = "http://www.inkscape.org/namespaces/inkscape";
pub const SODIPODI: &str = "http://sodipodi.sourceforge.net/DTD/sodipodi-0.dtd";
pub const MAX_VALUE_BYTES: usize = 1_024;
pub const MAX_OBJECT_ATTRIBUTES: usize = 16;
pub const MAX_OBJECT_BYTES: usize = 4_096;
pub const MAX_SCENE_BYTES: usize = 65_536;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SvgAttribute {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prefix: Option<String>,
    pub name: String,
    pub value: String,
}

fn xml_name(name: &str, limit: usize) -> bool {
    name.len() <= limit
        && name
            .as_bytes()
            .first()
            .is_some_and(|c| c.is_ascii_alphabetic() || *c == b'_')
        && name
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-' | b'.'))
}

fn admitted(namespace: Option<&str>, name: &str) -> bool {
    match namespace {
        Some(INKSCAPE) => matches!(
            name,
            "label" | "connector-curvature" | "groupmode" | "version"
        ),
        Some(SODIPODI) => name == "docname",
        None => name.strip_prefix("data-").is_some_and(|tail| {
            !tail.is_empty()
                && name != "data-name"
                && !tail.split(['-', '_', '.']).any(|part| {
                    matches!(
                        part,
                        "href" | "src" | "style" | "script" | "onload" | "onclick" | "onerror"
                    )
                })
        }),
        _ => false,
    }
}

pub fn from_xml(
    namespace: Option<&str>,
    prefix: Option<&str>,
    name: &str,
    value: &str,
) -> Result<Option<SvgAttribute>> {
    let candidate = matches!(namespace, Some(INKSCAPE | SODIPODI))
        || (namespace.is_none() && name.starts_with("data-") && name != "data-name");
    if !candidate {
        return Ok(None);
    }
    // Admission precedes allocation, even though the outer XML is also bounded.
    ensure!(
        value.len() <= MAX_VALUE_BYTES,
        "SVG metadata value exceeds 1 KiB"
    );
    ensure!(xml_name(name, 64), "Invalid SVG metadata attribute name");
    ensure!(
        prefix.is_none_or(|prefix| xml_name(prefix, 32)),
        "Invalid SVG metadata namespace prefix"
    );
    let attribute = SvgAttribute {
        namespace: namespace.map(str::to_owned),
        prefix: prefix.map(str::to_owned),
        name: name.to_owned(),
        value: value.to_owned(),
    };
    validate(std::slice::from_ref(&attribute))?;
    Ok(Some(attribute))
}

pub fn logical_bytes(attributes: &[SvgAttribute]) -> usize {
    attributes.iter().fold(0usize, |total, a| {
        total
            .saturating_add(a.name.len())
            .saturating_add(a.value.len())
            .saturating_add(a.namespace.as_ref().map_or(0, String::len))
            .saturating_add(a.prefix.as_ref().map_or(0, String::len))
    })
}

pub fn retained_bytes(attributes: &Vec<SvgAttribute>) -> usize {
    attributes
        .capacity()
        .saturating_mul(std::mem::size_of::<SvgAttribute>())
        .saturating_add(
            attributes
                .iter()
                .map(|a| {
                    a.name
                        .capacity()
                        .saturating_add(a.value.capacity())
                        .saturating_add(a.namespace.as_ref().map_or(0, String::capacity))
                        .saturating_add(a.prefix.as_ref().map_or(0, String::capacity))
                })
                .sum::<usize>(),
        )
}

pub fn validate(attributes: &[SvgAttribute]) -> Result<usize> {
    ensure!(
        attributes.len() <= MAX_OBJECT_ATTRIBUTES,
        "SVG metadata exceeds 16 attributes per object"
    );
    let mut keys = BTreeSet::new();
    for attribute in attributes {
        ensure!(
            xml_name(&attribute.name, 64)
                && admitted(attribute.namespace.as_deref(), &attribute.name),
            "Unsupported inert SVG metadata attribute {}",
            attribute.name
        );
        ensure!(
            attribute.namespace.is_some() || attribute.prefix.is_none(),
            "SVG metadata prefix has no namespace"
        );
        ensure!(
            attribute
                .prefix
                .as_ref()
                .is_none_or(|prefix| xml_name(prefix, 32)),
            "Invalid SVG metadata namespace prefix"
        );
        ensure!(
            attribute.value.len() <= MAX_VALUE_BYTES,
            "SVG metadata value exceeds 1 KiB"
        );
        ensure!(
            attribute
                .value
                .chars()
                .all(|c| matches!(c, '\t' | '\n' | '\r')
                    || ('\u{20}'..='\u{d7ff}').contains(&c)
                    || ('\u{e000}'..='\u{fffd}').contains(&c)
                    || c >= '\u{10000}'),
            "SVG metadata contains invalid XML characters"
        );
        let lower = attribute.value.to_ascii_lowercase();
        ensure!(
            ![
                "url(",
                "url (",
                "@import",
                "javascript:",
                "vbscript:",
                "data:"
            ]
            .iter()
            .any(|part| lower.contains(part)),
            "SVG metadata resource or script expressions are unsupported"
        );
        ensure!(
            keys.insert((attribute.namespace.as_deref(), attribute.name.as_str())),
            "Duplicate inert SVG metadata attribute"
        );
    }
    let bytes = logical_bytes(attributes);
    ensure!(
        bytes <= MAX_OBJECT_BYTES,
        "SVG metadata exceeds 4 KiB per object"
    );
    Ok(bytes)
}

fn escaped(value: &str) -> String {
    value
        .chars()
        .map(|c| match c {
            '&' => "&amp;".into(),
            '<' => "&lt;".into(),
            '>' => "&gt;".into(),
            '"' => "&quot;".into(),
            '\'' => "&apos;".into(),
            '\t' => "&#9;".into(),
            '\n' => "&#10;".into(),
            '\r' => "&#13;".into(),
            c => c.to_string(),
        })
        .collect()
}

/// Declarations are local to this leaf, preventing collisions with surrounding
/// SVG, paint and clip namespaces. Prefix text is a hint; namespace URI is identity.
pub fn export_attributes(attributes: &[SvgAttribute]) -> Result<String> {
    validate(attributes)?;
    let mut prefixes: BTreeMap<&str, String> = BTreeMap::new();
    for attribute in attributes {
        let Some(namespace) = attribute.namespace.as_deref() else {
            continue;
        };
        if prefixes.contains_key(namespace) {
            continue;
        }
        let wanted = attribute
            .prefix
            .as_deref()
            .unwrap_or(if namespace == INKSCAPE {
                "inkscape"
            } else {
                "sodipodi"
            });
        let base = if wanted.to_ascii_lowercase().starts_with("xml")
            || matches!(wanted, "svg" | "xlink" | "omuse")
        {
            "ometa"
        } else {
            wanted
        };
        let mut chosen = base.to_owned();
        let mut suffix = 2;
        while prefixes.values().any(|prefix| prefix == &chosen) {
            chosen = format!("{base}{suffix}");
            suffix += 1;
        }
        prefixes.insert(namespace, chosen);
    }
    let mut result = String::new();
    for (namespace, prefix) in &prefixes {
        result.push_str(&format!(" xmlns:{prefix}=\"{}\"", escaped(namespace)));
    }
    for attribute in attributes {
        let prefix = attribute
            .namespace
            .as_deref()
            .map(|ns| format!("{}:", prefixes[ns]))
            .unwrap_or_default();
        result.push_str(&format!(
            " {prefix}{}=\"{}\"",
            attribute.name,
            escaped(&attribute.value)
        ));
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn metadata_limits_and_forbidden_semantics_are_explicit() {
        assert!(
            from_xml(None, None, "data-tag", "Studio")
                .unwrap()
                .is_some()
        );
        assert!(
            from_xml(None, None, "data-name", "Owned display name")
                .unwrap()
                .is_none()
        );
        for (name, value) in [
            ("data-href", "file:///tmp/a"),
            ("data-tag", "javascript:alert(1)"),
            ("data-tag", "url(https://example.org/a)"),
            ("data-onload", "alert(1)"),
            ("data-tag", "\0"),
        ] {
            assert!(from_xml(None, None, name, value).is_err());
        }
        assert!(from_xml(Some(INKSCAPE), Some("i"), "original-d", "M0 0H2").is_err());
        assert!(from_xml(None, None, "data-tag", &"x".repeat(MAX_VALUE_BYTES + 1)).is_err());
        let attribute = from_xml(None, None, "data-tag", "ok").unwrap().unwrap();
        assert!(validate(&[attribute.clone(), attribute]).is_err());
    }
    #[test]
    fn namespace_collisions_and_xml_whitespace_round_trip() {
        let attributes = vec![
            from_xml(Some(INKSCAPE), Some("same"), "label", "A & \"B\" <C>\n\t")
                .unwrap()
                .unwrap(),
            from_xml(Some(SODIPODI), Some("same"), "docname", "Omuse.svg")
                .unwrap()
                .unwrap(),
        ];
        let encoded = format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\"><path{}/></svg>",
            export_attributes(&attributes).unwrap()
        );
        let parsed = roxmltree::Document::parse(&encoded).unwrap();
        let node = parsed
            .descendants()
            .find(|node| node.has_tag_name("path"))
            .unwrap();
        assert_eq!(
            node.attribute((INKSCAPE, "label")),
            Some(attributes[0].value.as_str())
        );
        assert_eq!(node.attribute((SODIPODI, "docname")), Some("Omuse.svg"));
    }
}
