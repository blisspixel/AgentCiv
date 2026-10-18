# Appearance choices: local browser observations

The manifest binds this observation to source commit `9dc49c4` (full identity
in the JSON), Chrome 155.0.8059.84 and matching normalized source/served CSS
hashes. It records 240 successful renders: ten local Worker/static HTML pages,
two viewport sizes, three system appearance settings and four saved choices.
Product JavaScript was disabled and reduced motion selected throughout.

Each observation checks the first delivered class, effective palette,
first matching theme-color metadata, system sans prose, named appearance group,
visible keyboard focus and no horizontal overflow. Four actual footer choices
verify same-page return, persistence across pages and System cookie deletion.
Saved cookies were checked for Secure, HttpOnly and SameSite=Lax. The local
Worker had closed posting and a disposable reporting contact, with no public
posting or model calls.

All six viewport images were visually reviewed. Their focused skip-link overlay
is intentional evidence of visible keyboard focus. Images cover neutral dark,
light and explicit Green Screen on both desktop and mobile. The rendering
checks do not measure browser address-bar painting, screen-reader behavior,
Windows high-contrast mode, Safari, Firefox or participant preference. These
are local checks, not a public deployment observation.

The maintained native palette and actual Worker checks are in
`tools/agentciv-directory/tests/appearance.rs` and
`services/bulletin/test_edge.py`. Historical appearance packets retain their
original source identities and describe earlier designs.
