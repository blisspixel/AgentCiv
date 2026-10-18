use std::collections::BTreeMap;

fn linear(hex: &str) -> [f64; 3] {
    let channel = |start| {
        let value = f64::from(u8::from_str_radix(&hex[start..start + 2], 16).unwrap()) / 255.0;
        if value <= 0.04045 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    };
    [channel(1), channel(3), channel(5)]
}

fn contrast(a: &str, b: &str) -> f64 {
    let luminance = |hex| {
        let [r, g, b] = linear(hex);
        0.2126 * r + 0.7152 * g + 0.0722 * b
    };
    let (a, b) = (luminance(a), luminance(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

#[test]
fn both_system_palettes_have_readable_text_controls_selection_and_calm_accent() {
    let css = include_str!("../../../website/style.css");
    let mut palettes = Vec::new();
    for block in css
        .split("::selection")
        .next()
        .unwrap()
        .split(":root {")
        .skip(1)
    {
        let (tokens, _) = block.split_once('}').unwrap();
        let palette: BTreeMap<_, _> = tokens
            .lines()
            .filter_map(|line| {
                let (name, value) = line.trim().trim_end_matches(';').split_once(": ")?;
                name.starts_with("--").then_some((name, value))
            })
            .collect();
        assert_eq!(palette.len(), 8);
        for background in ["--bg", "--surface"] {
            for foreground in ["--text", "--muted", "--accent"] {
                assert!(
                    contrast(palette[foreground], palette[background]) >= 4.5,
                    "{foreground} on {background}"
                );
            }
            assert!(contrast(palette["--border"], palette[background]) >= 3.0);
        }
        assert!(contrast(palette["--on-accent"], palette["--accent"]) >= 4.5);
        assert!(contrast(palette["--text"], palette["--selection"]) >= 4.5);
        palettes.push(palette);
    }
    assert_eq!(palettes.len(), 2);
    let [r, g, b] = linear(palettes[0]["--accent"]);
    let l = (0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b).cbrt();
    let m = (0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b).cbrt();
    let s = (0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b).cbrt();
    let a = 1.9779984951 * l - 2.4285922050 * m + 0.4505937099 * s;
    let b = 0.0259040371 * l + 0.7827717662 * m - 0.8086757660 * s;
    assert!(a.hypot(b) <= 0.16);
    let rules = css.split("::selection").nth(1).unwrap();
    assert!(
        !rules.contains('#'),
        "colour literals belong in palette blocks"
    );
    assert!(css.contains("@media (prefers-color-scheme: light)"));
    assert!(css.contains("@media (prefers-reduced-motion: reduce)"));
    assert!(css.contains(".brand-mark { color: var(--accent)"));
}

#[test]
fn generated_pages_and_marks_follow_system_appearance_without_scripts() {
    let pages = [
        include_str!("../../../website/index.template.html"),
        include_str!("../../../website/connect.html"),
        include_str!("../../../website/worlds.template.html"),
        include_str!("../../../website/resources.template.html"),
        include_str!("../../../website/research.html"),
        include_str!("../../../website/terms.html"),
        include_str!("../../../website/privacy.html"),
        include_str!("../../../website/404.html"),
    ];
    for page in pages {
        assert!(page.contains("content=\"#0e1511\""));
        assert!(page.contains("content=\"#f6f8f6\" media=\"(prefers-color-scheme: light)\""));
        assert!(!page.contains("<script"));
        if page.contains("brand-mark") {
            assert!(page.contains("stroke=\"currentColor\""));
        }
    }
    for svg in [
        include_str!("../../../website/favicon.svg"),
        include_str!("../../../website/logo.svg"),
    ] {
        assert!(svg.contains("prefers-color-scheme: light"));
        assert!(svg.contains("currentColor"));
        assert!(!svg.contains("#b6ff76"));
    }
}
