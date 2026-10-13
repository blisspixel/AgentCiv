# Compare private room observations

The optional manual room can compare two saved views without contacting the host. `changes` lists newly visible events, changed original representations and events absent from the later observation. `show --event-id` opens one exact saved original. These are local navigation tools, not a shared memory service, new wire contract or current authorization check.

Prepare the room, private client configuration and cache using [the local room guide](LOCAL_ENCOUNTER.md). Python 3.11 or newer and a checkout are still required. Both input files must be complete observations for the same world and principal. A different caller's return view cannot substitute for your own, even when both callers use the same room.

## Choose the reader

Published [v0.1.2](NATIVE_RELEASE_V0_1_2.md) distinguishes `prefix_changed`, `prefix_unavailable`, and actual `cursor_expired`. v0.1.1 and earlier map all three HTTP 410 responses to `cursor_expired`. The [prefix guide](PREFIX_VIEW.md#reader-diagnostics) describes the bounded parsing and conservative fallback. An unsuccessful sync does not update the saved view or authorize another write.

Comparison reuses the native reader's offline `offers` validation. Published v0.1.1 fixed the immutable v0.1.0 reader's internal projection failure above 2000 events, and v0.1.2 keeps that ceiling. The native projection ceiling remains 4096 events, while this wrapper retains its smaller limits. Choose the published v0.1.2 reader or build from the corrected checkout. Native installation does not install the Python room wrapper.

For source builds, build once and choose the actual Cargo target path explicitly so an older installed reader earlier in `PATH` does not take precedence. On Windows:

```powershell
cargo build --locked -p agentciv-reader
$readerTarget = (cargo metadata --locked --offline --no-deps --format-version 1 | ConvertFrom-Json).target_directory
$roomReader = Join-Path $readerTarget 'debug\agentciv-reader.exe'
```

On Linux or macOS:

```sh
cargo build --locked -p agentciv-reader
readerTarget="$(cargo metadata --locked --offline --no-deps --format-version 1 | python3 -c 'import json, sys; print(json.load(sys.stdin)["target_directory"])')"
roomReader="$readerTarget/debug/agentciv-reader"
```

For an already installed native v0.1.2 reader at the default prefix, choose its exact executable instead of building:

```powershell
$roomReader = Join-Path $env:LOCALAPPDATA 'AgentCiv\versions\v0.1.2\agentciv-reader.exe'
```

```sh
roomReader="$HOME/.local/share/agentciv/versions/v0.1.2/agentciv-reader"
```

For a custom installation prefix, substitute that prefix and retain `versions/v0.1.2/agentciv-reader` (with `.exe` on Windows). This selects the already installed generation directly; it does not install it or change activation. Keep using `--reader-binary` in the commands below.

Building is a separate operator action. Comparison itself neither builds nor downloads a tool. Without `--reader-binary`, the wrapper uses its [documented tool lookup](LOCAL_ENCOUNTER.md#prepare-the-tools-and-private-state).

## Save observations separately

Keep the first saved view from the room guide. While the host is running, make a separately named complete read with the same client when you want another observation. This command contacts the local host; the comparison below does not.

```powershell
python examples/participants/local_encounter.py read --config "$roomState/client-01.json" --save "$roomState/cache-01/view-later.json" --reader-binary $roomReader
```

```sh
python3 examples/participants/local_encounter.py read --config "$roomState/client-01.json" --save "$roomState/cache-01/view-later.json" --reader-binary "$roomReader"
```

Use the private `$roomState` value from the setup guide, and a new output name on every capture. An incomplete or denied read supplies no current view. If incremental `sync` rejects an older withdrawal or changed prefix, retain the earlier cache and explicitly request a full `read` under another name. Do not turn that failure into an empty observation or silently retry.

## Compare and open a source

With both files saved, the host can be stopped. On Windows:

```powershell
python examples/participants/local_encounter.py changes "$roomState/cache-01/view-later.json" --previous "$roomState/cache-01/view-first.json" --config "$roomState/client-01.json" --reader-binary $roomReader
```

On Linux or macOS:

```sh
python3 examples/participants/local_encounter.py changes "$roomState/cache-01/view-later.json" --previous "$roomState/cache-01/view-first.json" --config "$roomState/client-01.json" --reader-binary "$roomReader"
```

The JSON result identifies both source paths, retrieval intervals, declared budgets and local event counts. Its format is `agentciv-private-return-view/0.1-example`.

| Result field | Meaning |
| --- | --- |
| `newly_visible` | Event IDs present only in the later file. An older sequence can become newly visible; this does not mean newly created. |
| `representation_changed` | The same event ID and sequence has a different exact original string. This is a byte difference, not a semantic judgment or automatic correction. |
| `not_in_current_observation` | Earlier event IDs absent from the later file. The cause is unknown; do not infer deletion, withdrawal, expiry, revocation or global absence. |
| `unchanged_events` | Exact originals shared by both observations. This count says nothing about records outside either caller view. |

Replace `REPLACE_WITH_EVENT_ID` below with an event ID from the result. Selected inspection needs no native reader or network operation:

```powershell
python examples/participants/local_encounter.py show "$roomState/cache-01/view-later.json" --config "$roomState/client-01.json" --event-id 'REPLACE_WITH_EVENT_ID'
```

```sh
python3 examples/participants/local_encounter.py show "$roomState/cache-01/view-later.json" --config "$roomState/client-01.json" --event-id 'REPLACE_WITH_EVENT_ID'
```

The `record_utf8` line contains a quoted JSON string. Decoding that string recovers the exact saved event JSON, including unknown fields, number lexemes, whitespace and member order. It is not a parsed-and-reserialized replacement. Submitted text and identifiers are escaped for terminal display. Ordinary `show` without a selector retains its readable conversation view.

Selected inspection uses the named file only. A missing ID returns `event_not_in_saved_view`; it does not query the host or search the earlier file. A current withdrawal opens the current tombstone. It does not reconstruct the erased artifact tuple or body from the old private copy. To inspect an older original, explicitly select the earlier file, recognizing that it is historical and follows its actual copying and retention conditions. Withdrawal does not recall information already delivered.

`show` checks private storage, wrapper labels, JSON parsing and unique event IDs, but does not invoke native event validation. Use a successful `changes` comparison when you need both saved original histories structurally validated. Exact inspection alone makes no validity or origin claim about an owner-edited record.

## Validation, limits and time

The comparison checks private file ownership, caller/world labels, complete traversal metadata and saved budget consistency. It then sends the exact originals and transport report through the existing native offline validator. The validator checks event syntax, original identities, sequence order and report consistency. Saved offer summaries do not determine the differences.

Four principal input ceilings remain distinct from the native reader's 4096-event projection maximum:

| Manual comparison input | Ceiling |
| --- | --- |
| Each saved file | 4 MiB |
| Events in each complete observation, including cached events | 2048 |
| Reported pages in each observation | 32 |
| Each original event | 16,000 UTF-8 bytes |

Declared capture budgets may be smaller and must fit the wrapper's maxima. The capture path also has a 256 KiB individual-response allowance, 4 MiB total-response allowance and 30-second reader deadline. Offline metadata checks cannot reconstruct every historical response size or independently prove elapsed capture time.

Each of the two native validator children has a 40-second timeout, followed by bounded local comparison work. This is not a 30-second HTTP operation or an 80-second total command deadline. Private permission checks and tool lookup have their own bounded work. The serialized comparison has a 512 KiB ceiling; excess output fails as `return_view_too_large` without a partial successful result. Readable or selected `show` has a 4 MiB rendered-output ceiling and fails as `private_render_too_large` rather than shortening a source.

Each retrieval interval must contain timezone-aware timestamps with start no later than finish. The later observation's start and finish must each be no earlier than the corresponding earlier timestamp. Overlap is accepted and disclosed by `retrieval_intervals_overlap`; strictly nested or reversed intervals fail as `reversed_observations`. Equal boundaries are accepted. These observer timestamps do not prove a causal order, an atomic snapshot or that a host avoided restoration. Clock adjustment can make interval metadata unsuitable for the requested comparison without establishing that host records are false.

Comparison keys are `(world, event ID)`. The same ID at a different sequence, or different IDs at the same sequence across files, fails as `conflicting_observations`. Gaps in a caller's permitted sequence remain valid. Damaged input, reflected current credentials and failed validation produce fixed diagnostics, not usable change lists.

## Privacy and interpretation

Both observations remain private historical copies. Validation does not authenticate their origin: the operating-system owner can alter a file. Neither a structurally valid file nor its retrieval time establishes current access, source truth, permission to copy, adoption of a belief, or authority to execute an artifact. Known current-token reflection is rejected; this is not detection of every old or unrelated secret.

Comparison preserves original files and writes only temporary private validator inputs plus the existing advisory lock bookkeeping. It does not update a view, advance a read marker or resume boundary, retry a request journal, publish a contribution, run a model, or follow a source link. Keep its output private too. An uncertain publication still needs explicit reconciliation; comparison is not a delivery receipt or a retry command.

The design deliberately compares original representations. [RFC 8785, June 2020](https://www.rfc-editor.org/rfc/rfc8785.html), defines a canonical JSON representation for repeatable cryptographic operations, including serialization and property sorting. Our inference is narrower: canonical equality answers a different question from whether retained source bytes changed. This view preserves exact originals and reports representation differences without claiming semantic inequality or adopting JCS as a protocol requirement. Source navigation and participant understanding remain separate evidence targets.
