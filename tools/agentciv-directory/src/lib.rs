//! Build a public static directory without running or contacting listed worlds.

use std::collections::HashSet;
use std::fs;
use std::io::Read;
use std::path::Path;

use serde::{Deserialize, Serialize};
use url::Url;

mod package;
mod pages;
pub use package::{checked_prebuilt_config, package};

const ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../website");
const SCHEMA: &str = include_str!("../../../website/directory.schema.json");
const RESOURCE_SCHEMA: &str = include_str!("../../../website/resources.schema.json");
const TEMPLATE: &str = include_str!("../../../website/index.template.html");
const USAGE: &str = "Usage: agentciv-directory check [--input FILE]\n       agentciv-directory build --output DIRECTORY [--input FILE]\n       agentciv-directory package --output FRESH_DIRECTORY [--input FILE]";
const MAX_CATALOG_BYTES: usize = 65_536;

/// Public catalog data, separate from every AgentCiv wire profile.
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Directory {
    schema_version: u32,
    updated: String,
    entries: Vec<Listing>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Listing {
    id: String,
    name: String,
    kind: Kind,
    availability: Availability,
    summary: String,
    interfaces: Vec<Interface>,
    source_url: String,
    guide_url: String,
    notes: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    discovery_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    join_url: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum Kind {
    Commons,
    Creative,
    Game,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum Availability {
    Local,
    Public,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum Interface {
    Http,
    Mcp,
    Cli,
}

impl Kind {
    fn label(self) -> &'static str {
        match self {
            Self::Commons => "Commons",
            Self::Creative => "Creative world",
            Self::Game => "Game world",
        }
    }
}

impl Interface {
    fn label(self) -> &'static str {
        match self {
            Self::Http => "HTTP",
            Self::Mcp => "MCP",
            Self::Cli => "CLI",
        }
    }
}

/// Orientation guides published beside the directory. A guide is not a mind classification.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Library {
    schema_version: u32,
    updated: String,
    purpose: String,
    guides: Vec<Guide>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Guide {
    id: String,
    title: String,
    reviewed: String,
    summary: String,
    prerequisites: Vec<String>,
    sources: Vec<Source>,
    copying: String,
    limitations: String,
    alternatives: Vec<String>,
    corrections: String,
    url: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Source {
    title: String,
    url: String,
}

fn https_url(value: &str) -> Result<(), String> {
    let url = Url::parse(value).map_err(|error| format!("invalid URL: {error}"))?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || value
            .chars()
            .any(|character| character.is_whitespace() || character.is_control())
    {
        return Err(
            "listing URLs must use HTTPS without embedded credentials or whitespace".into(),
        );
    }
    Ok(())
}

fn date(value: &str) -> bool {
    let parts: Vec<_> = value.split('-').collect();
    if parts.len() != 3 || parts[0].len() != 4 || parts[1].len() != 2 || parts[2].len() != 2 {
        return false;
    }
    let numbers: Result<Vec<u32>, _> = parts.iter().map(|part| part.parse()).collect();
    let Ok(numbers) = numbers else { return false };
    let (year, month, day) = (numbers[0], numbers[1], numbers[2]);
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let days = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    year > 0 && (1..=12).contains(&month) && (1..=days[(month - 1) as usize]).contains(&day)
}

/// Validate bounded catalog shape, URLs, dates, and public versus local entry points.
pub fn parse(bytes: &[u8]) -> Result<Directory, String> {
    if bytes.len() > MAX_CATALOG_BYTES {
        return Err("directory exceeds 65536 bytes".into());
    }
    let value: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
    let schema: serde_json::Value =
        serde_json::from_str(SCHEMA).map_err(|error| error.to_string())?;
    jsonschema::validator_for(&schema)
        .map_err(|error| error.to_string())?
        .validate(&value)
        .map_err(|error| error.to_string())?;
    let directory: Directory = serde_json::from_value(value).map_err(|error| error.to_string())?;
    if !date(&directory.updated) {
        return Err("updated must be a real YYYY-MM-DD calendar date".into());
    }
    let mut ids = HashSet::new();
    for entry in &directory.entries {
        if !ids.insert(&entry.id) {
            return Err(format!("duplicate listing ID: {}", entry.id));
        }
        for text in [&entry.name, &entry.summary, &entry.notes] {
            if text.trim().is_empty() || text.chars().any(char::is_control) {
                return Err(format!(
                    "{}: text must be nonempty without control characters",
                    entry.id
                ));
            }
        }
        for value in [&entry.source_url, &entry.guide_url]
            .into_iter()
            .chain(entry.discovery_url.as_ref())
            .chain(entry.join_url.as_ref())
        {
            https_url(value)?;
        }
        match entry.availability {
            Availability::Local if entry.discovery_url.is_some() || entry.join_url.is_some() => {
                return Err(format!(
                    "{}: local examples cannot advertise public entry points",
                    entry.id
                ));
            }
            Availability::Public if entry.join_url.is_none() => {
                return Err(format!(
                    "{}: public listings need a join URL explaining access",
                    entry.id
                ));
            }
            _ => {}
        }
        if entry.discovery_url.is_some() && !matches!(entry.kind, Kind::Commons) {
            return Err(format!(
                "{}: only commons listings can name an AgentCiv discovery URL",
                entry.id
            ));
        }
    }
    Ok(directory)
}

fn plain(id: &str, text: &str) -> Result<(), String> {
    if text.trim().is_empty() || text.chars().any(char::is_control) {
        return Err(format!(
            "{id}: text must be nonempty without control characters"
        ));
    }
    Ok(())
}

/// Validate the orientation catalog. It asks for no private state and classifies no mind.
pub fn parse_resources(bytes: &[u8]) -> Result<Library, String> {
    if bytes.len() > MAX_CATALOG_BYTES {
        return Err("orientation library exceeds 65536 bytes".into());
    }
    let value: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
    let schema: serde_json::Value =
        serde_json::from_str(RESOURCE_SCHEMA).map_err(|error| error.to_string())?;
    jsonschema::validator_for(&schema)
        .map_err(|error| error.to_string())?
        .validate(&value)
        .map_err(|error| error.to_string())?;
    let library: Library = serde_json::from_value(value).map_err(|error| error.to_string())?;
    if library.schema_version != 1 {
        return Err("schema_version must be 1".into());
    }
    if !date(&library.updated) {
        return Err("updated must be a real YYYY-MM-DD calendar date".into());
    }
    plain("purpose", &library.purpose)?;
    let mut ids = HashSet::new();
    for guide in &library.guides {
        if !ids.insert(guide.id.clone()) {
            return Err(format!("duplicate guide ID: {}", guide.id));
        }
    }
    for guide in &library.guides {
        if !date(&guide.reviewed) {
            return Err(format!(
                "{}: reviewed must be a real calendar date",
                guide.id
            ));
        }
        for text in [
            &guide.title,
            &guide.summary,
            &guide.copying,
            &guide.limitations,
        ] {
            plain(&guide.id, text)?;
        }
        for item in &guide.prerequisites {
            plain(&guide.id, item)?;
        }
        for source in &guide.sources {
            plain(&guide.id, &source.title)?;
            https_url(&source.url)?;
        }
        https_url(&guide.corrections)?;
        https_url(&guide.url)?;
        for alternative in &guide.alternatives {
            if alternative == &guide.id || !ids.contains(alternative) {
                return Err(format!(
                    "{}: alternative {alternative} must name another guide in this catalog",
                    guide.id
                ));
            }
        }
    }
    Ok(library)
}

fn escape(value: &str) -> String {
    value.chars().fold(String::new(), |mut result, character| {
        match character {
            '&' => result.push_str("&amp;"),
            '<' => result.push_str("&lt;"),
            '>' => result.push_str("&gt;"),
            '"' => result.push_str("&quot;"),
            '\'' => result.push_str("&#39;"),
            other => result.push(other),
        }
        result
    })
}

/// Render reviewed listings as escaped HTML; a listing is never a live health check.
pub fn render(directory: &Directory) -> String {
    render_directory(directory, TEMPLATE)
}

fn render_directory(directory: &Directory, template: &str) -> String {
    let public_count = directory
        .entries
        .iter()
        .filter(|entry| matches!(entry.availability, Availability::Public))
        .count();
    let message = if public_count == 0 {
        "No public hosts are listed yet. Start locally, or propose a host you operate."
    } else {
        "Public entries are operator declarations. Read each world's access rules before joining."
    };
    let cards: String = directory.entries.iter().map(|entry| {
        let availability = match entry.availability {
            Availability::Local => "Run locally",
            Availability::Public => "Public entry point",
        };
        let interfaces = entry.interfaces.iter().map(|interface| interface.label()).collect::<Vec<_>>().join(" / ");
        let mut links = format!("<a class=\"card-link\" href=\"{}\">Read the guide</a><a href=\"{}\">Source</a>", escape(&entry.guide_url), escape(&entry.source_url));
        if let Some(url) = &entry.join_url {
            links.push_str(&format!("<a href=\"{}\">Joining and access</a>", escape(url)));
        }
        if let Some(url) = &entry.discovery_url {
            links.push_str(&format!("<a href=\"{}\">World descriptor</a>", escape(url)));
        }
        format!("<article class=\"world\" id=\"{}\"><div class=\"world-meta\"><span>{}</span><span>{}</span></div><h3>{}</h3><p>{}</p><p class=\"interfaces\">{}</p><p class=\"world-note\">{}</p><div class=\"world-links\">{links}</div></article>", escape(&entry.id), entry.kind.label(), availability, escape(&entry.name), escape(&entry.summary), interfaces, escape(&entry.notes))
    }).collect();
    let cards = if cards.is_empty() {
        "<p class=\"empty\">No worlds have been listed. Propose the first one below.</p>".to_owned()
    } else {
        cards
    };
    pages::fill(
        template,
        &[
            ("UPDATED", &escape(&directory.updated)),
            ("PUBLIC_COUNT", &public_count.to_string()),
            ("LISTING_COUNT", &directory.entries.len().to_string()),
            ("PUBLIC_MESSAGE", message),
            ("LISTINGS", &cards),
        ],
    )
}

/// Write only static public assets. Private history, credentials, and models are not read.
pub fn build(directory: &Directory, output: &Path) -> Result<(), String> {
    let resources = include_str!("../../../website/resources.json");
    let library = parse_resources(resources.as_bytes())?;
    fs::create_dir_all(output).map_err(|error| error.to_string())?;
    let json = serde_json::to_string_pretty(directory).map_err(|error| error.to_string())?;
    let html = render(directory);
    let worlds = render_directory(
        directory,
        include_str!("../../../website/worlds.template.html"),
    );
    let guides = pages::resources(&library);
    for (name, value) in [
        ("index.html", html.as_str()),
        (
            "connect.html",
            include_str!("../../../website/connect.html"),
        ),
        ("worlds.html", worlds.as_str()),
        ("resources.html", guides.as_str()),
        (
            "research.html",
            include_str!("../../../website/research.html"),
        ),
        ("directory.json", json.as_str()),
        ("directory.schema.json", SCHEMA),
        ("resources.json", resources),
        ("resources.schema.json", RESOURCE_SCHEMA),
        ("agent.json", include_str!("../../../website/agent.json")),
        (
            "bulletin-submit.schema.json",
            include_str!("../../../website/bulletin-submit.schema.json"),
        ),
        ("terms.html", include_str!("../../../website/terms.html")),
        (
            "privacy.html",
            include_str!("../../../website/privacy.html"),
        ),
        ("style.css", include_str!("../../../website/style.css")),
        ("favicon.svg", include_str!("../../../website/favicon.svg")),
        ("logo.svg", include_str!("../../../website/logo.svg")),
        ("_headers", include_str!("../../../website/_headers")),
        ("llms.txt", include_str!("../../../website/llms.txt")),
        (
            "robots.txt",
            "User-agent: *\nAllow: /\nSitemap: https://agentciv.io/sitemap.xml\n",
        ),
        (
            "sitemap.xml",
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?><urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\"><url><loc>https://agentciv.io/</loc></url><url><loc>https://agentciv.io/connect</loc></url><url><loc>https://agentciv.io/worlds</loc></url><url><loc>https://agentciv.io/resources</loc></url><url><loc>https://agentciv.io/research</loc></url></urlset>\n",
        ),
        ("404.html", include_str!("../../../website/404.html")),
    ] {
        fs::write(output.join(name), value).map_err(|error| error.to_string())?;
    }
    fs::create_dir_all(output.join(".well-known")).map_err(|error| error.to_string())?;
    fs::write(
        output.join(".well-known/agentciv-services"),
        include_str!("../../../website/agent.json"),
    )
    .map_err(|error| error.to_string())?;
    fs::create_dir_all(output.join("schemas/0.1-draft")).map_err(|error| error.to_string())?;
    fs::write(
        output.join("schemas/0.1-draft/message.schema.json"),
        include_str!("../../../schemas/message.schema.json"),
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

fn read_catalog(reader: impl Read) -> Result<Directory, String> {
    let mut bytes = Vec::new();
    reader
        .take((MAX_CATALOG_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    parse(&bytes)
}

/// Run the bounded check, static build, or prebuilt package without network requests.
pub fn run(args: &[String]) -> Result<String, String> {
    let Some(command) = args
        .first()
        .filter(|command| ["check", "build", "package"].contains(&command.as_str()))
    else {
        return Err(USAGE.into());
    };
    let mut input = format!("{ROOT}/directory.json");
    let mut output = None;
    let mut seen = HashSet::new();
    let (options, remainder) = args[1..].as_chunks::<2>();
    for pair in options {
        if !seen.insert(&pair[0]) || pair[1].is_empty() {
            return Err(USAGE.into());
        }
        match pair[0].as_str() {
            "--input" => input.clone_from(&pair[1]),
            "--output" if command != "check" => output = Some(&pair[1]),
            _ => return Err(USAGE.into()),
        }
    }
    if !remainder.is_empty() || (command != "check" && output.is_none()) {
        return Err(USAGE.into());
    }
    let input = fs::File::open(&input).map_err(|error| error.to_string())?;
    let directory = read_catalog(input)?;
    let resources =
        fs::read(format!("{ROOT}/resources.json")).map_err(|error| error.to_string())?;
    let library = parse_resources(&resources)?;
    let service = Path::new(ROOT).join("../services/bulletin");
    let source_config = fs::read_to_string(service.join("wrangler.toml"))
        .map_err(|_| "bulletin source configuration could not be read")?;
    checked_prebuilt_config(&source_config)?;
    if let Some(output) = output {
        if command == "package" {
            package(
                &directory,
                &service.join("build"),
                &source_config,
                Path::new(output),
            )?;
            Ok(format!("Prebuilt website and Worker packaged in {output}"))
        } else {
            build(&directory, Path::new(output))?;
            Ok(format!("Static directory built in {output}"))
        }
    } else {
        Ok(format!(
            "{} directory listings and {} orientation guides passed validation",
            directory.entries.len(),
            library.guides.len()
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct OversizedInput {
        received: usize,
    }

    impl Read for OversizedInput {
        fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
            if self.received == MAX_CATALOG_BYTES + 1 {
                return Err(std::io::Error::other("read beyond catalog limit"));
            }
            let length = buffer.len().min(MAX_CATALOG_BYTES + 1 - self.received);
            buffer[..length].fill(b' ');
            self.received += length;
            Ok(length)
        }
    }

    #[test]
    fn input_limit_stops_reading_before_consuming_the_rest_of_a_large_source() {
        let mut input = OversizedInput { received: 0 };
        assert_eq!(
            read_catalog(&mut input).unwrap_err(),
            "directory exceeds 65536 bytes"
        );
        assert_eq!(input.received, MAX_CATALOG_BYTES + 1);
    }

    #[test]
    fn catalog_at_exact_input_limit_is_accepted_and_io_failures_remain_errors() {
        let mut input = br#"{"schema_version":1,"updated":"2026-10-03","entries":[]}"#.to_vec();
        input.resize(MAX_CATALOG_BYTES, b' ');
        assert!(read_catalog(input.as_slice()).is_ok());
        struct Broken;
        impl Read for Broken {
            fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
                Err(std::io::Error::other("unavailable input"))
            }
        }
        assert_eq!(read_catalog(Broken).unwrap_err(), "unavailable input");
    }
}
