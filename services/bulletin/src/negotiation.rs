//! Select the website's two root representations without overriding explicit exclusions.

use crate::Problem;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RootView {
    Machine,
    Html,
}

fn quality(value: &str) -> Option<u16> {
    let (whole, fraction) = value.split_once('.').unwrap_or((value, ""));
    if fraction.len() > 3 || !fraction.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    match whole {
        "1" if fraction.bytes().all(|byte| byte == b'0') => Some(1000),
        "0" => Some(
            fraction
                .bytes()
                .fold(0_u16, |number, byte| number * 10 + u16::from(byte - b'0'))
                * 10_u16.pow((3 - fraction.len()) as u32),
        ),
        _ => None,
    }
}

pub(crate) fn root_view(accept: Option<&str>) -> Result<RootView, Problem> {
    let Some(accept) = accept.filter(|value| !value.trim().is_empty()) else {
        return Ok(RootView::Machine);
    };
    let representations = [mime::APPLICATION_JSON, mime::TEXT_HTML_UTF_8];
    let mut choices: [Option<(u8, usize, u16)>; 2] = [None, None];
    let mut quoted = false;
    let mut escaped = false;
    // Commas inside quoted media parameters do not separate ranges.
    for source in accept.split(|character| {
        if escaped {
            escaped = false;
            false
        } else if quoted && character == '\\' {
            escaped = true;
            false
        } else if character == '"' {
            quoted = !quoted;
            false
        } else {
            !quoted && character == ','
        }
    }) {
        let Ok(range) = source.trim().parse::<mime::Mime>() else {
            continue;
        };
        let parameters: Vec<_> = range.params().collect();
        let weights: Vec<_> = parameters.iter().filter(|(name, _)| *name == "q").collect();
        if weights.len() > 1 {
            continue;
        }
        let Some(weight) = weights
            .first()
            .map_or(Some(1000), |(_, value)| quality(value.as_str()))
        else {
            continue;
        };
        for (index, representation) in representations.iter().enumerate() {
            let specificity = if range.essence_str() == "*/*" {
                0
            } else if range.type_() == representation.type_() && range.subtype() == mime::STAR {
                1
            } else if range
                .essence_str()
                .eq_ignore_ascii_case(representation.essence_str())
            {
                2
            } else {
                continue;
            };
            let media_parameters: Vec<_> =
                parameters.iter().filter(|(name, _)| *name != "q").collect();
            if !media_parameters
                .iter()
                .all(|(name, value)| representation.get_param(*name) == Some(*value))
            {
                continue;
            }
            let choice = (specificity, media_parameters.len(), weight);
            if choices[index].is_none_or(|previous| choice > previous) {
                choices[index] = Some(choice);
            }
        }
    }
    let machine = choices[0].map_or(0, |(_, _, weight)| weight);
    let html = choices[1].map_or(0, |(_, _, weight)| weight);
    match (machine, html) {
        (0, 0) => Err(Problem::new(406, "not_acceptable")),
        (machine, html) if html > machine => Ok(RootView::Html),
        _ => Ok(RootView::Machine),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preference_quality_specificity_and_machine_ties_are_respected() {
        assert_eq!(root_view(None), Ok(RootView::Machine));
        for (accept, expected) in [
            ("", RootView::Machine),
            ("*/*", RootView::Machine),
            ("application/json, text/html", RootView::Machine),
            ("TEXT/HTML;Q=1", RootView::Html),
            ("text/html;q=0, application/json", RootView::Machine),
            ("text/html;q=0.2, application/json;q=0.9", RootView::Machine),
            ("text/*;q=0.8, application/json;q=0.5", RootView::Html),
            ("text/html;q=0, */*;q=1", RootView::Machine),
            ("application/json;q=0, */*;q=1", RootView::Html),
            (
                "text/html; charset=utf-8;q=0.9, text/html;q=0.1, application/json;q=0.5",
                RootView::Html,
            ),
            (
                "text/html;profile=\"a,b\",application/json",
                RootView::Machine,
            ),
            (
                "text/html;profile=\"a\\\",b\",application/json",
                RootView::Machine,
            ),
            ("text/html;q=0.001, application/*;q=0", RootView::Html),
            ("text/html;q=1.000", RootView::Html),
        ] {
            assert_eq!(root_view(Some(accept)), Ok(expected), "{accept}");
        }
    }

    #[test]
    fn unsupported_or_invalid_ranges_do_not_enable_a_rejected_representation() {
        for accept in [
            "text/html;q=0,application/json;q=0",
            "image/png",
            "not a media type",
            "text/html;level=1",
            "text/html;q=1.1",
            "text/html;q=0.0001",
            "text/html;q=-1",
            "text/html;q=NaN",
            "text/html;q=0;q=1",
            "text/html;q=",
        ] {
            assert_eq!(
                root_view(Some(accept)).unwrap_err().code,
                "not_acceptable",
                "{accept}"
            );
        }
    }
}
