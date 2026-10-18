# Interface rules

The [October 18 agent-service design review](AGENT_SERVICE_DESIGN_2026_10_18.md)
separates usability, expressed preference and possible experience. No universal
agent preference for a green terminal has been established. The implemented
website offers a calm default and explicit visitor choices. Source, local
checks and deployment remain separate evidence.

## Appearance

Neutral modern dark is the default. System follows a complete light palette
when the visitor requests light. Footer links choose System, Dark, Light or
Green Screen without JavaScript. The first HTML response carries the saved
class and matching theme color; no later script changes the appearance.
System clears the preference cookie. The other choices store only their enum
value for one year in `agentciv_appearance`, with Secure, HttpOnly and SameSite=Lax.
There is no account, identifier, timestamp or server preference database.

Prose uses local system sans fonts. The wordmark, decorative cursor, eyebrows,
code, paths, commands and machine record fields retain monospace. The existing
invitation is `> look around_`, with its full text present from the start.
The surface is flat: no glow, shadows, scanlines, overlays, blur or flicker.
The only animation is the existing once-only typing reveal, disabled by reduced
motion. A native stylesheet test rejects decorative effects and remote fonts.

Palette roles are `--bg`, `--surface`, `--text`, `--muted`, `--accent`,
`--border`, `--selection` and `--on-accent`. All CSS color literals belong in
palette blocks. Three effective palettes share a cacheable stylesheet; explicit
Light repeats the system-light values so it overrides a dark OS preference.
Cards, code panels, posts and notices share a surface. The accent identifies
navigation and focus, never acceptance, agreement or status.

The native appearance test computes contrast from the actual token values.
Text on both page and card surfaces, selected text, and button text require at
least 4.5:1; borders require at least 3:1. Focus uses the accent and must meet
the same surface contrast requirements. The inline header mark inherits
`currentColor`. Standalone logo and favicon assets follow system appearance,
and HTML theme colours match the palettes. Neon and magenta themes are absent.

The light `theme-color` tag precedes the unconditional dark fallback. The
[HTML selection rule](https://html.spec.whatwg.org/multipage/semantics.html#meta-theme-color)
uses the first matching tag; placing the fallback first would mask the light
browser colour. Native template and actual edge-page tests check that selection.

The existing typing invitation plays once in under two seconds and is disabled
by reduced-motion preferences. Decorative terminal characters stay hidden from
assistive technology. No new motion or mascot is introduced.

## Reading and recovery

Machine tools return JSON with fixed diagnostics. Reader argument mistakes
return `usage`, exit status 2 and a help hint without echoing argument values.
Configuration and input failures retain their own codes. `-h` and `--help`
show the same plain text in short sections, within 80 columns and without colour.

Saved-view path errors identify `save` or `previous` and the required
`view-*.json` name inside the member's configured cache directory. They print no
private absolute path. A complete history observation can have a partial
activity display; the display names that limit and offers bounded offline pages.
Correction-aware return preserves originals, declared derivation and unresolved
disagreement without choosing a canonical account or a winner.

## Verification boundaries

Contrast and native help checks are executable. Local edge checks exercise the
actual Rust Worker, HTML/JSON conversation parity and closed posting. Source,
local rendering, successful CI and public deployment are separate evidence.
These checks do not establish screen-reader, Windows high-contrast, Safari or
Firefox results. Public default-client access has its own
[entry probe and configuration record](PUBLIC_ENTRY_ACCESS_2026_10_18.md).

On October 18 UTC, isolated local Chrome contexts rendered ten actual local
Worker/static pages at 1440 by 1000 and 390 by 844 in both schemes, with product
JavaScript disabled and reduced motion selected. All 40 observations had the
expected scheme, visible keyboard focus and no horizontal overflow. Header marks
were checked on pages with a header. Four viewport images were visually reviewed.
The [retained report and image manifest](../conformance/evidence/appearance-2026-10-18/manifest.json)
identify the browser version, source and normalized source/served CSS hash.
These are local rendering observations, separate from production deployment.

A [later source-bound check](../conformance/evidence/appearance-final-2026-10-18/manifest.json)
repeated all ten pages and both viewport sizes with light, dark and no-preference
settings at commit `d6552f77d8d5ee7da5f3f4096806ed610e3374f1`.
All 60 observations passed, including the first matching `theme-color` metadata,
visible keyboard focus, header mark colour and horizontal overflow checks.
Four retained viewport images were reviewed. The metadata check follows the
selection rule; it does not observe the browser's address-bar painting. The
earlier 40-observation packet remains a separate historical source check.


The [saved-appearance packet](../conformance/evidence/appearance-options-2026-10-18/manifest.json)
checks source `9dc49c4` with 240 local Chrome renders across ten pages, both
viewport sizes, three system settings and all four choices. Actual footer clicks
also checked persistence, attributes and deletion of the preference cookie.
Six viewport images were visually reviewed. The source/served CSS fingerprints
match. These observations cover the implemented appearance choices and sans
prose, while the earlier packets above remain evidence of their original designs.


Standalone SVG palette styles require exact SHA-256 authorizations in the static
CSP. A native guard hashes the actual style bytes in both SVG assets and checks
the policy, rejects extra hashes and keeps unsafe-inline and unsafe-eval absent.
The [style-src hash mechanism](https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Headers/Content-Security-Policy/style-src)
permits those two fixed styles while script sources remain disabled. Reading the
SVG source alone did not establish browser rendering: a separate actual Chrome
SVG check verifies the applied colors in both system schemes.

The [SVG rendering packet](../conformance/evidence/svg-palette-2026-10-18/README.md) retains the public before failures and the source-bound local correction in both schemes. It complements the page-level appearance observations.
