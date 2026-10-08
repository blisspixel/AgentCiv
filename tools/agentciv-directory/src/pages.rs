//! Static inspection views of the validated catalogs. No network probes or scripts.

use crate::{Library, escape};

/// Substitute only markers in the original template, never markers in catalog text.
pub(super) fn fill(template: &str, values: &[(&str, &str)]) -> String {
    let mut parts = template.split("{{");
    let mut result = parts.next().unwrap_or_default().to_owned();
    for part in parts {
        if let Some((key, tail)) = part.split_once("}}")
            && let Some((_, value)) = values.iter().find(|(name, _)| *name == key)
        {
            result.push_str(value);
            result.push_str(tail);
        } else {
            result.push_str("{{");
            result.push_str(part);
        }
    }
    result
}

pub(super) fn resources(library: &Library) -> String {
    let mut contents = String::new();
    let mut cards = String::new();
    for guide in &library.guides {
        contents.push_str(&format!(
            "<li><a href=\"#{}\">{}</a></li>",
            escape(&guide.id),
            escape(&guide.title)
        ));
        let prerequisites: String = guide
            .prerequisites
            .iter()
            .map(|item| format!("<li>{}</li>", escape(item)))
            .collect();
        let sources: String = guide
            .sources
            .iter()
            .map(|source| {
                format!(
                    "<li><a href=\"{}\">{}</a></li>",
                    escape(&source.url),
                    escape(&source.title)
                )
            })
            .collect();
        let alternatives: String = guide
            .alternatives
            .iter()
            .map(|id| {
                let title = library
                    .guides
                    .iter()
                    .find(|other| &other.id == id)
                    .map_or(id.as_str(), |other| other.title.as_str());
                format!("<li><a href=\"#{}\">{}</a></li>", escape(id), escape(title))
            })
            .collect();
        cards.push_str(&format!(
            "<article class=\"guide-card\" id=\"{}\"><p class=\"eyebrow\">Reviewed <time datetime=\"{}\">{}</time></p><h2><a href=\"{}\">{}</a></h2><p class=\"guide-summary\">{}</p><h3>Before you start</h3><ul>{prerequisites}</ul><h3>Limits</h3><p>{}</p><h3>Copying conditions</h3><p>{}</p><div class=\"guide-links\"><div><h3>Original sources</h3><ul>{sources}</ul></div><div><h3>Related guides</h3><ul>{alternatives}</ul></div></div><p><a href=\"{}\">Read the full guide</a> <span aria-hidden=\"true\">/</span> <a href=\"{}\">Suggest a correction</a></p></article>",
            escape(&guide.id), escape(&guide.reviewed), escape(&guide.reviewed),
            escape(&guide.url), escape(&guide.title), escape(&guide.summary),
            escape(&guide.limitations), escape(&guide.copying), escape(&guide.url),
            escape(&guide.corrections)
        ));
    }
    if cards.is_empty() {
        cards.push_str("<p>No guides are listed in this catalog.</p>");
    }
    fill(
        include_str!("../../../website/resources.template.html"),
        &[
            ("UPDATED", &escape(&library.updated)),
            ("GUIDE_COUNT", &library.guides.len().to_string()),
            ("PURPOSE", &escape(&library.purpose)),
            ("CONTENTS", &contents),
            ("GUIDES", &cards),
        ],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn substitutions_never_execute_nested_markers_and_preserve_unknown_text() {
        assert_eq!(
            fill(
                "{{A}} {{B}} {{unknown}} {{unfinished",
                &[("A", "{{B}}"), ("B", "literal")]
            ),
            "{{B}} literal {{unknown}} {{unfinished"
        );
        assert_eq!(fill("", &[]), "");
    }

    #[test]
    fn guide_views_escape_all_untrusted_fields_and_resolve_local_alternatives() {
        let mut library =
            crate::parse_resources(include_bytes!("../../../website/resources.json")).unwrap();
        let unsafe_text = "<script>\"&' {{GUIDES}}";
        let guide = &mut library.guides[0];
        guide.title = unsafe_text.into();
        guide.summary = unsafe_text.into();
        guide.copying = unsafe_text.into();
        guide.limitations = unsafe_text.into();
        guide.prerequisites = vec![unsafe_text.into()];
        guide.sources[0].title = unsafe_text.into();
        guide.sources[0].url = "https://example.org/?a=1&b=2".into();
        library.purpose = unsafe_text.into();
        let html = resources(&library);
        assert!(!html.contains("<script>"));
        assert!(
            html.matches("&lt;script&gt;&quot;&amp;&#39; {{GUIDES}}")
                .count()
                >= 8
        );
        assert!(html.contains("?a=1&amp;b=2"));
        for guide in &library.guides {
            assert!(html.contains(&format!("id=\"{}\"", guide.id)));
            for alternative in &guide.alternatives {
                assert!(html.contains(&format!("href=\"#{alternative}\"")));
            }
        }
        library.guides.clear();
        assert!(resources(&library).contains("No guides are listed"));
    }
}
