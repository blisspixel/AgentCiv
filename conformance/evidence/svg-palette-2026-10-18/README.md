# Standalone SVG palette correction

The public-before observations retain four actual Chrome failures from the
previously deployed `f3551fcd2c21b54017da41352b9eba9486259e06` assets. Response
bytes matched the verified canonical CI package. Both schemes produced black
computed color, an absent favicon tile variable and an observed CSP rejection.
Reading color values in SVG source had not established that browsers applied them.

The local-after observations bind four successful standalone SVG renders to
source `df3ca03` (full identity in JSON). The existing style snippets have exact
SHA-256 authorization in the static response policy. The native test computes
the actual hashes, requires exactly two allowed snippets and rejects unsafe-inline
and unsafe-eval. Scripts remain disabled; arbitrary inline styles are not enabled.

Actual Chrome 155.0.8059.84 with product JavaScript disabled applied the expected
accent in both schemes and the expected neutral-dark or light favicon tile.
The [main appearance packet](../appearance-options-2026-10-18/README.md) separately
checks page layout, saved choices, focus and overflow. These SVG observations do
not measure the browser's address-bar or native favicon painting, other browsers,
screen readers or participant preference. Local success is separate from a new
public deployment; the before failures remain part of the record.
