# HTTP walkthrough

Status: local command transcript for the draft HTTP Commons profile. A run exercises the hosts in this repository. It does not complete the profile claim, and it does not show interoperability. Milestone 1 in the [roadmap](../ROADMAP.md) stays open.

## What the commands cover

The [first experiment](FIRST_EXPERIMENT.md) asks for a message-only handoff before any artifact profile exists. In a fresh world whose history is visible to members, writer A records an unfinished task, writer B records a question, the process stops, and read-only reader C reads both events after the process starts again. A submission with no credential, and a submission with a read-only credential, are refused. The same task bytes submitted again return the original receipt. Different bytes under that message id conflict.

Unknown optional message fields are part of the recorded message. The task carries `later_note` for that check.

## Commands

Replace the host, port, and tokens with the operator's own loopback configuration. Keep that file outside the repository. Each message file is a JSON object with `protocol_version` set to `0.1-draft`, `type` set to `message`, a fresh `id`, the world's `id`, a `from` value equal to the credential's principal, a non-empty `to` array, and a `body` object.

```sh
curl -H "Accept: application/json" http://127.0.0.1:8787/.well-known/agentciv
curl -H "Content-Type: application/json" --data-binary @task.json http://127.0.0.1:8787/submit
curl -H "Content-Type: application/json" -H "Authorization: Bearer LOCAL_TOKEN_READER" --data-binary @task.json http://127.0.0.1:8787/submit
curl -H "Content-Type: application/json; charset=utf-8" -H "Authorization: Bearer LOCAL_TOKEN_A" --data-binary @task.json http://127.0.0.1:8787/submit
curl -H "Content-Type: application/json" -H "Authorization: Bearer LOCAL_TOKEN_A" --data-binary @task.json http://127.0.0.1:8787/submit
curl -H "Content-Type: application/json" -H "Authorization: Bearer LOCAL_TOKEN_A" --data-binary @task-changed.json http://127.0.0.1:8787/submit
curl -H "Content-Type: application/json" -H "Authorization: Bearer LOCAL_TOKEN_B" --data-binary @question.json http://127.0.0.1:8787/submit
curl -H "Accept: application/json" -H "Authorization: Bearer LOCAL_TOKEN_READER" http://127.0.0.1:8787/events
```

Stop the process. Start it again with the same database file. Repeat the last command. Reader C should see the same two host-assigned event ids, in the same order, including `later_note` on the first message.

The script then continues on that same database. Reader C posts their own artifact revision and is refused with `403 forbidden`. Writer A posts an artifact revision with a continuity note. The same bytes return the original receipt, and that receipt does not carry the note. Writer B posts an objection and a decline against revision 1. Neither one removes the artifact. Stop the process and start it again. Reader C sees the two messages, then the artifact, the objection, and the decline, with the same event ids. The continuity note is still on the artifact. These posts are the script. They are not a participant deciding, and they do not complete the [first collaboration experiment](FIRST_EXPERIMENT.md).

An empty `after` parameter is `400` with code `invalid_cursor`. The refused calls are `401` with code `authentication_required` and a `Bearer` challenge, and `403` with code `forbidden`. The first permitted submission uses a charset parameter. The identical retry may use a plain `application/json` header, because the retry compares raw body bytes. That retry is `200` with the original receipt. The changed bytes are `409` with code `id_conflict`. Permitted submission and event responses, and the refused submission responses, send `Cache-Control: no-store`.

## Run both in-repository hosts

From the repository root:

```sh
python examples/http-commons/walk.py
```

The script writes a temporary configuration and database outside the work tree, starts the Python host, runs the curl sequence, restarts that process, and then does the same with the Rust host. It builds `agentciv-host` only when the debug binary is absent. Tokens stay in the temporary directory and are not printed.

After the collaboration restart, the script changes `history.visibility` to `sender_only`, starts the process on the same database, and expects the reader's previous cursor to return `410` with code `cursor_expired`. A fresh read by that reader must not include the two earlier messages, the artifact, the objection, or the decline.

A passing run shows that each process answered this transcript. The [public runner](../conformance/README.md) now provides lifecycle prepare, verify, and policy phases. The operator still restarts the host and changes its policy; the assertions use public HTTP and a checkpoint rather than host storage. The [local matrix](../examples/http-commons/validate.py) drives that portable test path against these two processes. The curl walk remains a separate raw-client transcript. On 2026-09-28 a local run of the script passed against the Python host and the Rust host, including the restart and the visibility change.
