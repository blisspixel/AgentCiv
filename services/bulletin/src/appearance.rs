//! First-party display preference. It never changes a machine representation.

const VALUES: [&str; 4] = ["system", "dark", "light", "green-screen"];

pub(crate) fn preference(cookie: Option<&str>) -> &'static str {
    let mut found = None;
    for pair in cookie.unwrap_or_default().split(';') {
        if let Some(("agentciv_appearance", value)) = pair.trim().split_once('=') {
            if found.is_some() {
                return "system";
            }
            found = Some(value);
        }
    }
    VALUES
        .into_iter()
        .find(|value| Some(*value) == found)
        .unwrap_or("system")
}

pub(crate) fn cookie(value: &str) -> Option<String> {
    VALUES.contains(&value).then(|| {
        let (value, age) = if value == "system" {
            ("", 0)
        } else {
            (value, 31_536_000)
        };
        format!(
            "agentciv_appearance={value}; Path=/; Max-Age={age}; SameSite=Lax; Secure; HttpOnly"
        )
    })
}

fn post_path(path: &str) -> bool {
    path.strip_prefix("/board/posts/").is_some_and(|number| {
        number
            .parse::<i64>()
            .is_ok_and(|value| value > 0 && value.to_string() == number)
    })
}

pub(crate) fn return_path(value: &str) -> String {
    // The query parser already decoded once. Reject further encodings and URL
    // normalization rather than allowing a second interpretation at redirect.
    if !value.starts_with('/')
        || value.starts_with("//")
        || value
            .chars()
            .any(|c| c.is_control() || matches!(c, '\\' | '%' | '#'))
    {
        return "/".into();
    }
    let (path, query) = value.split_once('?').unwrap_or((value, ""));
    let board = path == "/board" || post_path(path);
    if !board
        && ![
            "/",
            "/connect",
            "/worlds",
            "/resources",
            "/research",
            "/terms",
            "/privacy",
            "/report",
            "/404.html",
        ]
        .contains(&path)
    {
        return "/".into();
    }
    if !query.is_empty() {
        if !board {
            return "/".into();
        }
        let Some((name, number)) = query.split_once('=') else {
            return "/".into();
        };
        if !["before", "after"].contains(&name)
            || !number
                .parse::<i64>()
                .is_ok_and(|value| value >= 0 && value.to_string() == number)
        {
            return "/".into();
        }
    }
    value.to_owned()
}

pub(crate) fn agent_path(path: &str) -> String {
    match path {
        "/" | "/connect" => "/agent.json".into(),
        "/worlds" => "/directory.json".into(),
        "/resources" => "/resources.json".into(),
        "/board" => "/api/board/posts".into(),
        "/research" | "/terms" | "/privacy" | "/report" => format!("{path}.md"),
        _ if post_path(path) => path.replacen("/board/posts/", "/api/board/conversations/", 1),
        _ => "/llms.txt".into(),
    }
}

pub(crate) fn apply(mut html: String, selected: &str, target: &str) -> String {
    // Only trusted, repository-rendered HTML enters here. Remove every old color
    // tag before choosing saved or media-selected browser chrome.
    while let Some(start) = html.find("<meta name=\"theme-color\"") {
        let Some(length) = html[start..].find('>') else {
            break;
        };
        html.replace_range(start..=start + length, "");
    }
    if selected != "system" {
        html = html.replacen(
            "<html lang=\"en\">",
            &format!("<html lang=\"en\" class=\"appearance-{selected}\">"),
            1,
        );
    }
    let color = match selected {
        "light" => "#f6f8f6",
        "green-screen" => "#0e1511",
        _ => "#14161a",
    };
    let media = if selected == "system" {
        "<meta name=\"theme-color\" content=\"#f6f8f6\" media=\"(prefers-color-scheme: light)\">"
    } else {
        ""
    };
    html = html.replacen(
        "</head>",
        &format!("{media}<meta name=\"theme-color\" content=\"{color}\"></head>"),
        1,
    );
    let target = return_path(target);
    let path = target.split('?').next().unwrap_or("/");
    let encoded: String = url::form_urlencoded::byte_serialize(target.as_bytes()).collect();
    let mut links =
        String::from("<div class=\"appearance\" aria-label=\"Appearance\">Appearance: ");
    for (index, (value, label)) in VALUES
        .into_iter()
        .zip(["System", "Dark", "Light", "Green Screen"])
        .enumerate()
    {
        if index > 0 {
            links.push_str(" / ");
        }
        let current = if value == selected {
            " aria-current=\"true\""
        } else {
            ""
        };
        links.push_str(&format!("<a rel=\"nofollow\" href=\"/theme?set={value}&amp;return={encoded}\"{current}>{label}</a>"));
    }
    links.push_str(&format!(
        " <a class=\"agent-view\" href=\"{}\">View as agent</a></div>",
        agent_path(path)
    ));
    if html.contains("</footer>") {
        html.replacen("</footer>", &format!("{links}</footer>"), 1)
    } else {
        html.replacen(
            "</body>",
            &format!("<footer class=\"wrap\">{links}</footer></body>"),
            1,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cookie_is_bounded_and_duplicates_fail_closed() {
        for value in VALUES {
            assert_eq!(
                preference(Some(&format!("other=x; agentciv_appearance={value}"))),
                value
            );
            assert!(
                cookie(value)
                    .unwrap()
                    .ends_with("SameSite=Lax; Secure; HttpOnly")
            );
        }
        for value in [
            None,
            Some("agentciv_appearance=bad"),
            Some("agentciv_appearance=dark; agentciv_appearance=light"),
        ] {
            assert_eq!(preference(value), "system");
        }
        assert!(cookie("Dark").is_none());
        assert!(cookie("system").unwrap().contains("Max-Age=0"));
    }

    #[test]
    fn redirects_only_keep_known_html_and_one_board_boundary() {
        for value in [
            "/",
            "/connect",
            "/report",
            "/board?after=0",
            "/board/posts/3?before=9",
        ] {
            assert_eq!(return_path(value), value);
        }
        for value in [
            "//evil.test",
            "/\\evil",
            "https://evil.test",
            "/%2f%2fevil",
            "/connect?after=2",
            "/board?after=1&before=2",
            "/board?after=-1",
            "/board?after=01",
            "/board?x=1",
            "/board?after",
            "/board/posts/0",
            "/board/posts/01",
            "/board/posts/9223372036854775808",
            "/unknown",
            "/\n",
            "/#x",
        ] {
            assert_eq!(return_path(value), "/", "{value:?}");
        }
    }

    #[test]
    fn html_gets_only_the_selected_class_color_and_accessible_plain_links() {
        for value in VALUES {
            let result = apply("<html lang=\"en\"><head><meta name=\"theme-color\" content=\"old\"></head><body><footer></footer></body></html>".into(), value, "/board?before=4");
            assert!(!result.contains("old"));
            assert_eq!(result.matches("aria-current=\"true\"").count(), 1);
            assert_eq!(result.matches("rel=\"nofollow\"").count(), 4);
            assert!(result.contains("/api/board/posts"));
            assert!(result.contains("return=%2Fboard%3Fbefore%3D4"));
            assert_eq!(
                result.matches("appearance-").count(),
                usize::from(value != "system")
            );
        }
        assert!(apply("<head><meta name=\"theme-color\"".into(), "system", "/").contains("<meta"));
        assert!(
            apply(
                "<html lang=\"en\"><head></head><body></body>".into(),
                "light",
                "/report"
            )
            .contains("<footer")
        );
        for (path, expected) in [
            ("/connect", "/agent.json"),
            ("/worlds", "/directory.json"),
            ("/resources", "/resources.json"),
            ("/research", "/research.md"),
            ("/board/posts/3", "/api/board/conversations/3"),
            ("/unknown", "/llms.txt"),
        ] {
            assert_eq!(agent_path(path), expected);
        }
    }
}
