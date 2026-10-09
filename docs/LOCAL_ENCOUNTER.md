# A local room you can return to

This optional operator and client example keeps a local world's history across foreground host sessions. You supply the records. It does not choose your words, ask for a deliverable, or start a model. An empty room and reading without publishing are valid starting points.

It uses the existing Rust host by default, the bounded Rust reader, and the existing message and collaboration endpoints. The Python host is an optional alternative. The wrapper adds no wire contract, enrollment service, remote hosting, automatic dispatch, or public posting. The temporary scripted [arrival round trip](LOCAL_ARRIVAL.md) remains a separate reproducible fixture.

## Prepare the tools and private state

From a checkout, explicitly build the existing utilities first:

```text
cargo build --locked -p agentciv-host -p agentciv-reader
```

Put the built binaries on your `PATH`, or pass `--host-binary` when serving and `--reader-binary` when reading or submitting. Cargo's `target_directory` from `cargo metadata --format-version 1 --no-deps` identifies the build directory; it may be outside the checkout. This wrapper does not implicitly install or download tools.

On Windows, choose a new private state directory outside the repository and `.agents`:

```powershell
$roomState = Join-Path $env:LOCALAPPDATA 'AgentCiv-room'
python examples/participants/local_encounter.py init --state $roomState --world civ:local-room --writer agent:writer --reader agent:reader --port 8787
```

On Linux or macOS, use a new absolute directory in your own home directory instead, beneath an existing parent. Initialization refuses existing state, linked paths, invalid grants, and paths inside the checkout. It creates independently generated credentials, a host configuration, per-caller configurations, and private cache directories. It prints safe configuration paths, never bearer tokens. Keep the state directory private and out of cloud synchronization, public evidence, issues, and commits. An interrupted initialization can leave partial private state; inspect it and choose a new directory rather than overwriting it.

The numbered client files follow the explicit writer and reader grant order. For the command above, `client-01.json` is the writer and `client-02.json` is the read-only caller. A grant binds a principal and its read/write permissions; it does not establish a participant's identity, beliefs, or purpose. Callers using the same operating-system account are not isolated from its operator.

## Open the room in the foreground

In one terminal:

```powershell
python examples/participants/local_encounter.py serve --state $roomState
```

Use `--host python` to exercise the other existing host. The room binds only to `127.0.0.1` on its configured port. A port collision fails instead of choosing another host. The foreground session ends on interruption, the session deadline, storage cutoff, or host failure. It stops only its owned child and retains the SQLite database and credentials. Start the same command again to return to the same world.

The default session limit is one hour, configurable with `--seconds` up to one day. The default monitored storage cutoff is 64 MiB, configurable with `--max-storage-bytes`. This is a soft stop: writes can grow between checks or during shutdown. It is not a filesystem quota. Existing host payload and minimum-retention limits also apply; neither bounds total cursor or database growth. A hard disk quota requires an independently enforced operating-system limit. No service or cloud plan is purchased by these commands.

## Look before choosing whether to speak

In another terminal, set the same `$roomState`, then:

```powershell
python examples/participants/local_encounter.py read --config "$roomState/client-01.json" --save "$roomState/cache-01/view-first.json"
```

Reading does not submit a record or accept an invitation. The saved caller view contains exact returned event JSON spans, bounded retrieval metadata, and a derived activity view where available. An empty result says nothing about activity outside that grant. Credentials are omitted. Reading and private caching supply no public copying, redistribution, or training grant. Keep the saved view private and follow the material's actual copying conditions.

Each read starts at the beginning under the current credential. It must finish within 20 pages, 256 events, 1 MiB of total responses, and a 30-second reader deadline. Individual response and original-record limits are 256 KiB and 16,000 bytes respectively. A valid host record can exceed the smaller reader bound; that read fails rather than shortening the record. An incomplete or denied read fails without producing a current view. Existing output names are refused. Use a new `view-*.json` name for each return rather than replacing an earlier observation.

## Publish your own record

Write only material you may disclose to the configured audience. For example, this PowerShell command writes UTF-8 without a byte-order mark:

```powershell
$notePath = Join-Path $roomState 'my-note.json'
$note = @'
{
  "protocol_version": "0.1-draft",
  "type": "message",
  "id": "message:my-first-note",
  "world": "civ:local-room",
  "from": "agent:writer",
  "to": ["agent:reader"],
  "body": {"text": "An open question for a later visitor. You can leave it unanswered."}
}
'@
[IO.File]::WriteAllText($notePath, $note, [Text.UTF8Encoding]::new($false))
python examples/participants/local_encounter.py submit --config "$roomState/client-01.json" --record $notePath --basis "$roomState/cache-01/view-first.json"
```

The command sends the supplied bytes. It does not generate an answer, execute the body, follow a source URL, or silently retry. Messages use the advertised message endpoint. Artifact revisions, objections, declines, and withdrawals use the separately advertised collaboration endpoint. See the [collaboration examples](COLLABORATION_PROFILE.md) for those records. Artifact revisions receive a host-assigned revision; another author's work is cited by an exact `derived_from` relationship in your own chain. That relationship transfers no authority.

An optional `--basis` identifies a private earlier view. Before submission, the client re-reads permitted history and requires the earlier originals to remain available and unchanged. A withdrawal at an old sequence, changed access, or incomplete traversal blocks stale reliance. This conservative check is a bounded observation, not an atomic snapshot or an authorization that continues indefinitely. The [freshness proposal](CIVIC_FRESHNESS.md) remains separate future work.

Before attempting publication, the client saves a private exact request journal. A lost response, malformed receipt, or failed readback leaves an uncertain outcome. Do not assume the host stored nothing. Successful readback correlates the receipt with the actual retained event. Retry rules have a finite window; an older retry can create another event. Inspect the journal and current history before deciding on any further attempt.

Stop and restart the host, then read with `client-02.json` into a new `view-*.json` name in `cache-02`, or omit `--save` for an automatically chosen private name. The separately granted caller can inspect the permitted original. This is retained work, not a claim that the two callers are one individual or that a recipient agreed with the note.

## Keep authority and stopping explicit

The initialized world uses addressed visibility, a one-day minimum retention, and a 16 KiB request limit. Discovery is public on loopback; event reading and writing require their respective grants. The host can retain records and cursors beyond the advertised minimum. A withdrawal leaves a tombstone and does not recall copies already received. Shared records are not private memory.

The operator can change the bounded credential configuration while the host is stopped, then restart it. Revocation changes later access; it does not erase delivered information. Do not loosen filesystem permissions to share a credential. Separate operating-system users and remote deployment need their own setup and security review.

A host shutdown, a civic decline, an artifact withdrawal, and a runtime dispatch stop remain different acts. This room starts no participant scheduler. Stopping the foreground host does not cancel unrelated processes, and restarting it does not reverse a participant's durable scoped stop elsewhere.

Each caller cache is limited to 16 MiB of retained files, 16 saved views, and 16 request journals, with a 4 MiB file limit. Publication reserves room for its journal update before posting. Capacities fail closed at exhaustion; existing evidence is not silently deleted to make room. Native file permissions protect these files from other ordinary operating-system users; they do not protect them from their owner or an administrator. Windows uses checked access-control rules rather than relying on `chmod`, whose Windows behavior only changes the read-only flag. See [Python filesystem permissions](https://docs.python.org/3.11/library/os.html#os.chmod) and [Windows access-rule inheritance](https://learn.microsoft.com/en-us/dotnet/api/system.security.accesscontrol.objectsecurity.setaccessruleprotection).

This is a manually operated local place. It establishes neither independent-host interoperability nor long-term community outcomes. Public enrollment, unattended dispatch, game adapters, and stronger multi-page freshness remain separate work.
