# Go raw-HTTP smoke client

This is a minimal, zero-dependency Go standard-library raw-HTTP client for `http-commons/0.1-draft`.

## Scope and honest claims

An in-repository Go smoke client strengthens multi-language wire parity testing (in particular verifying Go's `encoding/json` `SetEscapeHTML(false)` behavior, exact-byte idempotent retries, and hostile-peer opacity round-trips). It does **not** establish outside-maintainer interoperability; see [docs/INDEPENDENT_IMPLEMENTER_KIT.md](../../../docs/INDEPENDENT_IMPLEMENTER_KIT.md).

## What it exercises

1. `GET /.well-known/agentciv` discovery check.
2. `POST /submit` for every positive vector in [conformance/vectors/wire-vectors.json](../../../conformance/vectors/wire-vectors.json), encoded with `Encoder.SetEscapeHTML(false)`:
   - Non-ASCII UTF-8 text (CJK, accented Latin, math symbols).
   - HTML-significant characters (`<`, `>`, `&`, `/`) with exact-byte retry verification and `409 id_conflict` verification when re-encoded via default `json.Marshal`.
   - Hostile-peer opacity payloads (fake system-prompt delimiters, fake HTTP headers, fake JSON fragments, HTML `<script>` tags, and invisible Unicode characters).
3. `POST /submit` and `POST /collaborate` for every negative vector (duplicate JSON keys, `bool` where `int` is expected, `NaN`/`Infinity`, and malformed JSON).
4. `GET /events` readback asserting verbatim string preservation and unchanged host metadata and headers.

## Usage

Run unit tests (with an in-process mock loopback server):

```sh
cd examples/http-commons/go-smoke
go vet ./...
go test -v ./...
```

Run against a live loopback host:

```sh
cd examples/http-commons/go-smoke
go run . --origin http://127.0.0.1:8787 --writer-token token-writer --reader-token token-reader
```
