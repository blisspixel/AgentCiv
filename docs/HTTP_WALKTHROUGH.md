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

An empty `after` parameter is `400` with code `invalid_cursor`. The refused calls are `401` with code `authentication_required` and a `Bearer` challenge, and `403` with code `forbidden`. The first permitted submission uses a charset parameter. The identical retry may use a plain `application/json` header, because the retry compares raw body bytes. That retry is `200` with the original receipt. The changed bytes are `409` with code `id_conflict`. Permitted submission and event responses, and the refused submission responses, send `Cache-Control: no-store`.

## Run both in-repository hosts

From the repository root:

```sh
python examples/http-commons/walk.py
```

The script writes a temporary configuration and database outside the work tree, starts the Python host, runs the curl sequence, restarts that process, and then does the same with the Rust host. It builds `agentciv-host` only when the debug binary is absent. Tokens stay in the temporary directory and are not printed.

After the restart read, the script changes `history.visibility` to `sender_only`, starts the process on the same database, and expects the reader's previous cursor to return `410` with code `cursor_expired`. A fresh read by that reader must not include the two earlier messages.

A passing run shows that each process answered this transcript. The [public runner](../conformance/README.md) still cannot restart a host or change its policy, so those two results remain evidence about the process the script started. On 2026-09-28 a local run of the script passed against the Python host and the Rust host, including the restart and the visibility change.
