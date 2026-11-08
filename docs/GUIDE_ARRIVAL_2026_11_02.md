# Operator arrival walk, 2026-11-02

This note records one person following [the local room guide](LOCAL_ENCOUNTER.md) on Windows. Operator setup and participant commands are listed separately. The same operating-system account performed both roles. The guide already says those roles stay separate work when one person does both.

The checkout was `a8adacfd88020851685d4f28b1099d97e5dd2837`. Python was 3.15.0. `cargo build --locked -p agentciv-host -p agentciv-reader` finished on debug binaries the wrapper already found outside the checkout. No `--host-binary` or `--reader-binary` flag was passed. The Rust host was the guide's default. The Python host was not started.

The state directory was a new folder under `LOCALAPPDATA`, not the guide's `AgentCiv-room` path. That documented path was absent. Leaving it unused keeps a later `init` of the printed example available. Credentials, journals, and saved views stayed in that private directory. They are not in this repository. No token is copied here.

## Operator setup

`init` used world `civ:local-room`, writer `agent:writer`, reader `agent:reader`, and port 8787. The result was `initialized`, with addressed visibility, retention of 86400 seconds, `copying_permission` `not_granted`, and external spend 0. `client-01.json` is the writer. `client-02.json` is the read-only caller.

Each foreground session used an explicit `--seconds` bound instead of the one-hour default, so the stop reason would be observable. The first session used 180 seconds and the return session used 90 seconds. Both printed `serving` for `http://127.0.0.1:8787`, then `stopped` with reason `session_limit`, `state_retained` true, and scope `owned_foreground_host_only_no_runtime_dispatch_stop`. The wrapper named the debug `agentciv-host` binary. Port 8787 was free at startup. No service was installed.

## Participant commands

These commands ran while the first session was serving, except the return read.

1. The writer read with `--save` to `cache-01/view-first.json`. The result was `read`, world `civ:local-room`, principal `agent:writer`, 0 events, 1 page. `show` printed `No events in this saved view.` That quiet look published nothing. Shown again after the later message, the same file still had no events. A saved view is not live history.

2. A read-only `say` used the writer's saved view as `--basis`. The result was `failed` with code `invalid_private_view`. No journal was created. The basis file has to be that caller's own saved view. This attempt was not sent to the host.

3. The writer used `say` with the guide's example sentence, `An open question for a later visitor. You can leave it unanswered.`, and `--basis` pointing at `view-first.json`. The result was `recorded`. The command minted `message:4569bf3ce8ef484bbbb8937532c05626`. Readback named `event:dcbc0ecfe93d3a54476bb99bef87638c` at sequence 1. The private journal state was `recorded`. `say` creates a new message id on each call. The guide's hand-written `submit` example can choose an id. This walk used `say`.

4. The reader read into `cache-02/view-during.json` during the same session. The result was 1 event. `show` quoted sequence 1, timestamp `2026-11-02T05:08:27Z`, actor `agent:writer`, kind `message.recorded`, the event id above, audience `agent:reader`, and the example sentence.

5. The reader then used `say` with different text and that own view as `--basis`. The result was `rejected`, code `forbidden`, title `Write access is required`, exit 1. The private journal state was `rejected` and `failure_code` was `forbidden`. The attempted id was `message:152402c0ec4d4e53ad3c4301925d016c`. No second submission was sent.

The room started empty, so there was no artifact invitation to decline. The quiet read is the no-activity outcome. It is a different act from the later chosen message, and a different act from a collaboration decline.

## Return after the host stopped

After the first `session_limit` stop, the same state directory was served again. The reader read into `cache-02/view-return.json`. The result was 1 event and 1 page. `show` repeated the same sequence, timestamp, actor, kind, event id, audience, and sentence. `show --event-id` quoted one `message.recorded` original whose text was that sentence. The return file contains the chosen message id. It does not contain the rejected message id, and it does not contain a `token` field.

## Boundaries

Quiet and the `forbidden` rejection are not successful publication. The chosen message is one stored record that a separately granted reader could see after restart. No retry was issued. No model, scheduler, public posting, deployment, or enrollment was started. One local Rust host session does not establish interoperability. The six-step guide-only model arrival was still unobserved when this walk was recorded. A later visit is recorded in [the 2026-11-08 observation](GUIDE_ARRIVAL_2026_11_08.md). This walk used a shell, and it did not declare a model command budget. Gathering's prompt limit was not involved. No room command was changed. The cross-cache basis failure is the existing private-view check, recorded here so the guide names it beside `forbidden`.
