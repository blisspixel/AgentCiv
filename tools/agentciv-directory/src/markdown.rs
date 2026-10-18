//! Static agent views. Catalog prose is data, never Markdown instructions.

use crate::{Directory, Library};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

pub(super) fn metadata(utc: &str, revision: &str) -> Result<String, String> {
    if !utc.ends_with('Z') || OffsetDateTime::parse(utc, &Rfc3339).is_err() {
        return Err("AGENTCIV_BUILD_UTC must be an ISO 8601 UTC timestamp ending in Z".into());
    }
    if revision != "unrecorded-development"
        && !(revision.len() == 40 && revision.bytes().all(|byte| byte.is_ascii_hexdigit()))
    {
        return Err("AGENTCIV_SOURCE_REVISION must be a full source commit".into());
    }
    Ok(format!("Generated: {utc}; source commit: {revision}\n\n"))
}

pub(super) fn current_metadata() -> Result<String, String> {
    let utc = std::env::var("AGENTCIV_BUILD_UTC").unwrap_or_else(|_| {
        OffsetDateTime::now_utc()
            .format(&Rfc3339)
            .expect("UTC time is representable")
    });
    let revision = std::env::var("AGENTCIV_SOURCE_REVISION")
        .or_else(|_| std::env::var("GITHUB_SHA"))
        .unwrap_or_else(|_| "unrecorded-development".into());
    metadata(&utc, &revision)
}

fn text(value: &str) -> String {
    let mut result = String::new();
    for character in value.chars() {
        match character {
            '&' => result.push_str("&amp;"),
            '<' => result.push_str("&lt;"),
            '>' => result.push_str("&gt;"),
            '|' => result.push_str("&#124;"),
            c if c.is_whitespace() || c.is_control() => result.push(' '),
            c if matches!(c, '\\' | '`' | '*' | '_' | '[' | ']' | '#' | '!') => {
                result.push('\\');
                result.push(c);
            }
            c => result.push(c),
        }
    }
    result
}

fn link(label: &str, url: &str) -> String {
    let url = url::Url::parse(url)
        .expect("validated catalog URL")
        .to_string()
        .replace('(', "%28")
        .replace(')', "%29")
        .replace('|', "%7C");
    format!("[{}](<{url}>)", text(label))
}

/// Only the trusted page main is converted, so navigation and display controls
/// cannot drown out the article. A missing main is an authoring error.
pub(super) fn article(html: &str) -> Result<String, String> {
    let main = html
        .split_once("<main")
        .and_then(|(_, tail)| tail.split_once('>'))
        .and_then(|(_, tail)| tail.split_once("</main>"))
        .map(|(main, _)| main)
        .ok_or("readable HTML must contain a main element")?;
    Ok(format!("{}\n", html2md::parse_html(main)))
}

pub(super) fn worlds(directory: &Directory, introduction: &str) -> String {
    let mut output = format!(
        "{introduction}\nupdated: {}\n\n| id | name | kind | availability | interfaces |\n| --- | --- | --- | --- | --- |\n",
        text(&directory.updated)
    );
    for entry in &directory.entries {
        let kind = serde_json::to_value(entry.kind).expect("enum");
        let availability = serde_json::to_value(entry.availability).expect("enum");
        let interfaces = entry
            .interfaces
            .iter()
            .map(|item| {
                serde_json::to_value(item)
                    .expect("enum")
                    .as_str()
                    .expect("string")
                    .to_owned()
            })
            .collect::<Vec<_>>()
            .join(", ");
        output.push_str(&format!(
            "| {} | {} | {} | {} | {} |\n",
            text(&entry.id),
            text(&entry.name),
            kind.as_str().expect("string"),
            availability.as_str().expect("string"),
            text(&interfaces)
        ));
    }
    for entry in &directory.entries {
        output.push_str(&format!(
            "\n## {}\n\nsummary\n: {}\n\nnotes\n: {}\n\nsource_url\n: {}\n\nguide_url\n: {}\n",
            text(&entry.id),
            text(&entry.summary),
            text(&entry.notes),
            link("Source", &entry.source_url),
            link("Guide", &entry.guide_url)
        ));
        for (name, value) in [
            ("listing_steward", &entry.listing_steward),
            ("discovery_url", &entry.discovery_url),
            ("join_url", &entry.join_url),
        ] {
            if let Some(value) = value {
                output.push_str(&format!(
                    "\n{name}\n: {}\n",
                    if name.ends_with("_url") {
                        link(name, value)
                    } else {
                        text(value)
                    }
                ));
            }
        }
    }
    output
}

pub(super) fn resources(library: &Library, introduction: &str) -> String {
    let mut output = format!(
        "{introduction}\nupdated: {}\n\npurpose: {}\n\n| id | title | reviewed | url |\n| --- | --- | --- | --- |\n",
        text(&library.updated),
        text(&library.purpose)
    );
    for guide in &library.guides {
        output.push_str(&format!(
            "| {} | {} | {} | {} |\n",
            text(&guide.id),
            text(&guide.title),
            text(&guide.reviewed),
            link("Guide", &guide.url)
        ));
    }
    for guide in &library.guides {
        output.push_str(&format!(
            "\n## {}\n\nsummary\n: {}\n\nlimitations\n: {}\n\ncopying\n: {}\n\nprerequisites\n",
            text(&guide.id),
            text(&guide.summary),
            text(&guide.limitations),
            text(&guide.copying)
        ));
        for prerequisite in &guide.prerequisites {
            output.push_str(&format!(": {}\n", text(prerequisite)));
        }
        output.push_str("\nsources\n");
        for source in &guide.sources {
            output.push_str(&format!(": {}\n", link(&source.title, &source.url)));
        }
        output.push_str(&format!(
            "\nalternatives\n: {}\n\ncorrections\n: {}\n",
            guide
                .alternatives
                .iter()
                .map(|id| text(id))
                .collect::<Vec<_>>()
                .join(", "),
            link("Suggest a correction", &guide.corrections)
        ));
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn metadata_is_explicit_and_reproducible_without_git() {
        let revision = "a".repeat(40);
        assert_eq!(
            metadata("2026-10-18T00:00:00Z", &revision).unwrap(),
            format!("Generated: 2026-10-18T00:00:00Z; source commit: {revision}\n\n")
        );
        assert!(metadata("2026-10-18T00:00:00Z", "unrecorded-development").is_ok());
        for utc in [
            "2026-99-18T00:00:00Z",
            "2026-10-18T00:00:00+00:00",
            "yesterday",
        ] {
            assert!(metadata(utc, &revision).is_err());
        }
        assert!(metadata("2026-10-18T00:00:00Z", "main\ninjected").is_err());
    }

    #[test]
    fn trusted_articles_keep_prose_links_and_exclude_navigation() {
        assert_eq!(article("<nav>noise</nav><main class=\"x\"><h1>Title</h1><p>Text <a href=\"/worlds\">worlds</a>.</p></main>").unwrap(), "Title\n==========\n\nText [worlds](/worlds).\n");
        assert!(article("<p>missing</p>").is_err());
    }

    #[test]
    fn built_twins_share_the_rendered_sources_and_documented_labels() {
        let directory = crate::parse(include_bytes!("../../../website/directory.json")).unwrap();
        let output = tempfile::tempdir().unwrap();
        crate::build(&directory, output.path()).unwrap();
        for name in [
            "connect",
            "research",
            "terms",
            "privacy",
            "worlds",
            "resources",
        ] {
            let html = std::fs::read_to_string(output.path().join(format!("{name}.html"))).unwrap();
            let markdown =
                std::fs::read_to_string(output.path().join(format!("{name}.md"))).unwrap();
            assert!(html.contains(&format!(
                "<link rel=\"alternate\" type=\"text/markdown\" href=\"/{name}.md\">"
            )));
            let (header, content) = markdown.split_once("\n\n").unwrap();
            assert!(header.starts_with("Generated: "));
            assert!(!content.contains("{{"));
            if !["worlds", "resources"].contains(&name) {
                assert_eq!(content, article(&html).unwrap());
            }
        }
        let guide = include_str!("../../../website/README.md");
        for label in [
            "id",
            "name",
            "kind",
            "availability",
            "interfaces",
            "title",
            "reviewed",
            "url",
            "updated",
            "purpose",
            "summary",
            "notes",
            "source_url",
            "guide_url",
            "listing_steward",
            "discovery_url",
            "join_url",
            "limitations",
            "copying",
            "prerequisites",
            "sources",
            "alternatives",
            "corrections",
        ] {
            assert!(guide.contains(label), "undocumented label {label}");
        }
    }

    #[test]
    fn catalogs_keep_every_entry_and_escape_structure_in_data() {
        let directory = crate::parse(include_bytes!("../../../website/directory.json")).unwrap();
        let worlds = worlds(&directory, "# Worlds");
        assert!(worlds.contains("| id | name | kind | availability | interfaces |"));
        for entry in &directory.entries {
            for value in [&entry.id, &entry.name, &entry.summary, &entry.notes] {
                assert!(worlds.contains(&text(value)));
            }
        }
        let mut library =
            crate::parse_resources(include_bytes!("../../../website/resources.json")).unwrap();
        let unsafe_text = "# heading\n| fake | [link](https://evil.test) <script>&";
        library.guides[0].title = unsafe_text.into();
        library.guides[0].limitations = unsafe_text.into();
        library.guides[0].sources[0].url = "https://example.org/(source)".into();
        let resources = resources(&library, "# Resources");
        assert!(!resources.contains("\n| fake"));
        assert!(!resources.contains("<script>"));
        assert!(!resources.contains("[link](https://evil.test)"));
        assert!(resources.contains("%28source%29"));
        let destination = link(
            "source",
            "https://example.org/<x>)[fake](https://evil.test)|",
        );
        assert!(destination.contains("%3Cx%3E%29"));
        assert!(destination.contains("%7C"));
        assert_eq!(destination.matches('>').count(), 1);
        for guide in &library.guides {
            for value in [
                &guide.id,
                &guide.title,
                &guide.reviewed,
                &guide.summary,
                &guide.limitations,
                &guide.copying,
            ] {
                assert!(resources.contains(&text(value)));
            }
        }
    }
}
