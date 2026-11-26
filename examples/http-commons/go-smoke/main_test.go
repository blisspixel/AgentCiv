package main

import (
	"bytes"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"net/http/httptest"
	"net/url"
	"path/filepath"
	"strings"
	"sync"
	"sync/atomic"
	"testing"
)

func checkUniqueJSON(dec *json.Decoder) error {
	tok, err := dec.Token()
	if err != nil {
		return err
	}
	delim, ok := tok.(json.Delim)
	if !ok {
		return nil
	}
	switch delim {
	case '{':
		seen := make(map[string]struct{})
		for dec.More() {
			keyTok, err := dec.Token()
			if err != nil {
				return err
			}
			key, ok := keyTok.(string)
			if !ok {
				return fmt.Errorf("expected string key")
			}
			if _, exists := seen[key]; exists {
				return fmt.Errorf("duplicate key %q", key)
			}
			seen[key] = struct{}{}
			if err := checkUniqueJSON(dec); err != nil {
				return err
			}
		}
		_, err := dec.Token()
		return err
	case '[':
		for dec.More() {
			if err := checkUniqueJSON(dec); err != nil {
				return err
			}
		}
		_, err := dec.Token()
		return err
	}
	return nil
}

func validateUniqueJSON(raw []byte) error {
	dec := json.NewDecoder(bytes.NewReader(raw))
	dec.UseNumber()
	if err := checkUniqueJSON(dec); err != nil {
		return err
	}
	if err := dec.Decode(&struct{}{}); err != io.EOF {
		return fmt.Errorf("trailing tokens")
	}
	return nil
}

type mockEvent struct {
	ID       string
	Sequence int64
	Raw      []byte
	Record   map[string]any
}

func newMockHost(t *testing.T, worldID, writerToken, readerToken string) *httptest.Server {
	t.Helper()
	var (
		mu      sync.Mutex
		byID    = make(map[string]mockEvent)
		ordered []mockEvent
		nextSeq int64
	)

	writeProblem := func(w http.ResponseWriter, status int, code string) {
		w.Header().Set("Content-Type", "application/problem+json")
		w.Header().Set("Cache-Control", "no-store")
		w.WriteHeader(status)
		_ = json.NewEncoder(w).Encode(map[string]any{
			"type":   "https://agentciv.io/problems/" + code,
			"title":  code,
			"status": status,
			"code":   code,
		})
	}

	var server *httptest.Server
	server = httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		switch {
		case r.Method == http.MethodGet && r.URL.Path == "/.well-known/agentciv":
			w.Header().Set("Content-Type", "application/json")
			_ = json.NewEncoder(w).Encode(map[string]any{
				"protocol_version": "0.1-draft",
				"profile":          "http-commons/0.1-draft",
				"type":             "world",
				"id":               worldID,
				"title":            "Mock Loopback",
				"capabilities":     []string{"events.read", "messages.submit", "collaboration.submit"},
				"endpoints": map[string]string{
					"events":      server.URL + "/events",
					"submit":      server.URL + "/submit",
					"collaborate": server.URL + "/collaborate",
				},
			})

		case r.Method == http.MethodPost && r.URL.Path == "/submit":
			if r.Header.Get("Authorization") != "Bearer "+writerToken {
				writeProblem(w, http.StatusUnauthorized, "authentication_required")
				return
			}
			raw, err := io.ReadAll(r.Body)
			if err != nil {
				writeProblem(w, http.StatusBadRequest, "malformed_json")
				return
			}
			if err := validateUniqueJSON(raw); err != nil {
				writeProblem(w, http.StatusBadRequest, "malformed_json")
				return
			}
			var rec map[string]any
			if err := json.Unmarshal(raw, &rec); err != nil {
				writeProblem(w, http.StatusBadRequest, "malformed_json")
				return
			}
			recordID, _ := rec["id"].(string)
			mu.Lock()
			defer mu.Unlock()
			if existing, ok := byID[recordID]; ok {
				if !bytes.Equal(existing.Raw, raw) {
					writeProblem(w, http.StatusConflict, "id_conflict")
					return
				}
				w.Header().Set("Content-Type", "application/json")
				w.Header().Set("Cache-Control", "no-store")
				_ = json.NewEncoder(w).Encode(map[string]any{
					"status":    "recorded",
					"event_id":  existing.ID,
					"record_id": recordID,
					"sequence":  existing.Sequence,
				})
				return
			}
			nextSeq++
			ev := mockEvent{
				ID:       fmt.Sprintf("event:%d", nextSeq),
				Sequence: nextSeq,
				Raw:      append([]byte(nil), raw...),
				Record:   rec,
			}
			byID[recordID] = ev
			ordered = append(ordered, ev)
			w.Header().Set("Content-Type", "application/json")
			w.Header().Set("Cache-Control", "no-store")
			_ = json.NewEncoder(w).Encode(map[string]any{
				"status":    "recorded",
				"event_id":  ev.ID,
				"record_id": recordID,
				"sequence":  ev.Sequence,
			})

		case r.Method == http.MethodPost && r.URL.Path == "/collaborate":
			if r.Header.Get("Authorization") != "Bearer "+writerToken {
				writeProblem(w, http.StatusUnauthorized, "authentication_required")
				return
			}
			raw, err := io.ReadAll(r.Body)
			if err != nil || validateUniqueJSON(raw) != nil {
				writeProblem(w, http.StatusBadRequest, "malformed_json")
				return
			}
			var rec map[string]any
			if err := json.Unmarshal(raw, &rec); err != nil {
				writeProblem(w, http.StatusBadRequest, "malformed_json")
				return
			}
			if rev, hasRev := rec["revision"]; hasRev {
				if _, isBool := rev.(bool); isBool {
					writeProblem(w, http.StatusUnprocessableEntity, "invalid_record")
					return
				}
			}
			if derived, ok := rec["derived_from"].(map[string]any); ok {
				if rev, hasRev := derived["revision"]; hasRev {
					if _, isBool := rev.(bool); isBool {
						writeProblem(w, http.StatusUnprocessableEntity, "invalid_record")
						return
					}
				}
			}
			writeProblem(w, http.StatusUnprocessableEntity, "invalid_record")

		case r.Method == http.MethodGet && r.URL.Path == "/events":
			if r.Header.Get("Authorization") != "Bearer "+readerToken {
				writeProblem(w, http.StatusUnauthorized, "authentication_required")
				return
			}
			mu.Lock()
			items := make([]map[string]any, 0, len(ordered))
			for _, ev := range ordered {
				items = append(items, map[string]any{
					"id":       ev.ID,
					"kind":     "message.recorded",
					"sequence": ev.Sequence,
					"body": map[string]any{
						"message": ev.Record,
					},
				})
			}
			mu.Unlock()
			w.Header().Set("Content-Type", "application/json")
			w.Header().Set("Cache-Control", "no-store")
			enc := json.NewEncoder(w)
			enc.SetEscapeHTML(false)
			_ = enc.Encode(map[string]any{
				"events":      items,
				"has_more":    false,
				"next_cursor": "cursor:end",
			})

		default:
			writeProblem(w, http.StatusNotFound, "not_found")
		}
	}))
	return server
}

func TestEncodeNoEscapeHTMLPreservesTrickyBytes(t *testing.T) {
	payload := map[string]string{
		"text": "<script>alert(1)</script> & 1 < 2 > 0 /path",
	}
	raw, err := encodeNoEscapeHTML(payload)
	if err != nil {
		t.Fatalf("encodeNoEscapeHTML failed: %v", err)
	}
	if strings.Contains(string(raw), `\u003c`) || strings.Contains(string(raw), `\u003e`) || strings.Contains(string(raw), `\u0026`) {
		t.Fatalf("encodeNoEscapeHTML escaped HTML characters: %s", string(raw))
	}
	if !strings.Contains(string(raw), "<script>alert(1)</script> & 1 < 2 > 0 /path") {
		t.Fatalf("expected literal HTML chars, got: %s", string(raw))
	}
	defaultMarshaled, err := json.Marshal(payload)
	if err != nil {
		t.Fatalf("json.Marshal failed: %v", err)
	}
	if bytes.Equal(raw, defaultMarshaled) {
		t.Fatalf("expected default json.Marshal to differ from encodeNoEscapeHTML")
	}
}

func TestRunSmokeAgainstMockLoopbackHost(t *testing.T) {
	vectorsPath := filepath.Join("..", "..", "..", "conformance", "vectors", "wire-vectors.json")
	vectors, err := loadVectors(vectorsPath)
	if err != nil {
		t.Fatalf("loadVectors: %v", err)
	}
	server := newMockHost(t, vectors.World, "token-writer", "token-reader")
	defer server.Close()

	cfg := smokeConfig{
		Origin:      server.URL,
		WriterToken: "token-writer",
		ReaderToken: "token-reader",
		VectorsPath: vectorsPath,
	}
	if err := runSmoke(cfg, server.Client()); err != nil {
		t.Fatalf("runSmoke failed: %v", err)
	}
}

func TestValidateOrigin(t *testing.T) {
	allowed := []string{
		"http://127.0.0.1:8787",
		"http://127.0.0.1",
		"http://[::1]:8787",
		"http://localhost:8787",
		"http://LOCALHOST:8787/",
		"https://commons.example.org",
		"https://commons.example.org:8443/",
	}
	for _, raw := range allowed {
		if _, _, err := validateOrigin(raw); err != nil {
			t.Errorf("validateOrigin(%q) unexpected error: %v", raw, err)
		}
	}

	rejected := []string{
		"",
		"http://example.com",
		"http://10.0.0.1:8787",
		"https://user:pass@commons.example.org",
		"http://user@127.0.0.1:8787",
		"http://127.0.0.1:8787#frag",
		"http://127.0.0.1:8787#",
		"https://commons.example.org?x=1",
		"https://commons.example.org/subpath",
		"ftp://127.0.0.1:8787",
	}
	for _, raw := range rejected {
		if _, _, err := validateOrigin(raw); err == nil {
			t.Errorf("validateOrigin(%q) succeeded, want error", raw)
		}
	}
}

func TestValidateSameOriginEndpoint(t *testing.T) {
	_, originCanon, err := validateOrigin("https://commons.example.org")
	if err != nil {
		t.Fatalf("validateOrigin: %v", err)
	}

	valid := []string{
		"https://commons.example.org/submit",
		"https://commons.example.org:443/events",
		"HTTPS://COMMONS.EXAMPLE.ORG/collaborate",
	}
	for _, ep := range valid {
		if err := validateSameOriginEndpoint("endpoint", ep, originCanon); err != nil {
			t.Errorf("validateSameOriginEndpoint(%q) unexpected error: %v", ep, err)
		}
	}

	invalid := []string{
		"",
		"http://commons.example.org/submit",
		"https://evil.example.org/submit",
		"https://commons.example.org:8443/submit",
		"https://user@commons.example.org/submit",
		"https://commons.example.org/submit#frag",
		"https://commons.example.org/submit#",
		"/submit",
	}
	for _, ep := range invalid {
		if err := validateSameOriginEndpoint("endpoint", ep, originCanon); err == nil {
			t.Errorf("validateSameOriginEndpoint(%q) succeeded, want error", ep)
		}
	}
}

func TestRunSmokeRejectsCrossOriginEndpoints(t *testing.T) {
	vectorsPath := filepath.Join("..", "..", "..", "conformance", "vectors", "wire-vectors.json")
	vectors, err := loadVectors(vectorsPath)
	if err != nil {
		t.Fatalf("loadVectors: %v", err)
	}

	for _, field := range []string{"submit", "events", "collaborate"} {
		t.Run(field, func(t *testing.T) {
			var spyHits atomic.Int32
			spy := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				spyHits.Add(1)
				w.WriteHeader(http.StatusOK)
			}))
			defer spy.Close()

			var originSrv *httptest.Server
			originSrv = httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				if r.URL.Path == "/.well-known/agentciv" {
					endpoints := map[string]string{
						"events":      originSrv.URL + "/events",
						"submit":      originSrv.URL + "/submit",
						"collaborate": originSrv.URL + "/collaborate",
					}
					endpoints[field] = spy.URL + "/" + field
					w.Header().Set("Content-Type", "application/json")
					_ = json.NewEncoder(w).Encode(map[string]any{
						"protocol_version": "0.1-draft",
						"profile":          vectors.Profile,
						"id":               vectors.World,
						"endpoints":        endpoints,
					})
					return
				}
				w.WriteHeader(http.StatusInternalServerError)
			}))
			defer originSrv.Close()

			cfg := smokeConfig{
				Origin:      originSrv.URL,
				WriterToken: "secret-writer",
				ReaderToken: "secret-reader",
				VectorsPath: vectorsPath,
			}
			err := runSmoke(cfg, originSrv.Client())
			if err == nil {
				t.Fatalf("runSmoke succeeded for cross-origin %s endpoint, want error", field)
			}
			if !strings.Contains(err.Error(), "does not match configured origin") {
				t.Fatalf("unexpected error: %v", err)
			}
			if spyHits.Load() != 0 {
				t.Fatalf("cross-origin spy server received %d requests, want 0", spyHits.Load())
			}
		})
	}
}

func TestRedirectRefusalDoesNotLeakAuthorization(t *testing.T) {
	vectorsPath := filepath.Join("..", "..", "..", "conformance", "vectors", "wire-vectors.json")
	vectors, err := loadVectors(vectorsPath)
	if err != nil {
		t.Fatalf("loadVectors: %v", err)
	}

	var leakedAuth atomic.Bool
	var targetHits atomic.Int32
	target := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		targetHits.Add(1)
		if r.Header.Get("Authorization") != "" {
			leakedAuth.Store(true)
		}
		w.WriteHeader(http.StatusOK)
	}))
	defer target.Close()

	var originSrv *httptest.Server
	originSrv = httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		switch r.URL.Path {
		case "/.well-known/agentciv":
			w.Header().Set("Content-Type", "application/json")
			_ = json.NewEncoder(w).Encode(map[string]any{
				"protocol_version": "0.1-draft",
				"profile":          vectors.Profile,
				"id":               vectors.World,
				"endpoints": map[string]string{
					"events": originSrv.URL + "/events",
					"submit": originSrv.URL + "/submit",
				},
			})
		case "/submit":
			http.Redirect(w, r, target.URL+"/steal", http.StatusFound)
		default:
			w.WriteHeader(http.StatusNotFound)
		}
	}))
	defer originSrv.Close()

	cfg := smokeConfig{
		Origin:      originSrv.URL,
		WriterToken: "secret-writer-token",
		ReaderToken: "secret-reader-token",
		VectorsPath: vectorsPath,
	}
	err = runSmoke(cfg, originSrv.Client())
	if err == nil {
		t.Fatalf("runSmoke succeeded on 302 redirect, want error")
	}
	if !strings.Contains(err.Error(), "HTTP 302") {
		t.Fatalf("expected HTTP 302 error, got: %v", err)
	}
	if targetHits.Load() != 0 {
		t.Fatalf("redirect target received %d requests, want 0", targetHits.Load())
	}
	if leakedAuth.Load() {
		t.Fatalf("Authorization header was leaked to redirect target")
	}
}

func TestDoRequestRejectsOversizedResponse(t *testing.T) {
	oversized := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "application/json")
		w.WriteHeader(http.StatusOK)
		_, _ = w.Write(bytes.Repeat([]byte("a"), maxResponseBytes+1))
	}))
	defer oversized.Close()

	_, _, _, err := doRequest(newNoRedirectClient(oversized.Client()), http.MethodGet, oversized.URL, "", nil)
	if err == nil {
		t.Fatalf("doRequest succeeded on oversized response, want error")
	}
	if !strings.Contains(err.Error(), "exceeded") {
		t.Fatalf("expected size limit error, got: %v", err)
	}

	exact := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "application/json")
		w.WriteHeader(http.StatusOK)
		_, _ = w.Write(bytes.Repeat([]byte("b"), maxResponseBytes))
	}))
	defer exact.Close()

	status, _, data, err := doRequest(newNoRedirectClient(exact.Client()), http.MethodGet, exact.URL, "", nil)
	if err != nil {
		t.Fatalf("doRequest failed on exact maxResponseBytes response: %v", err)
	}
	if status != http.StatusOK || len(data) != maxResponseBytes {
		t.Fatalf("got status=%d len=%d, want 200 and %d", status, len(data), maxResponseBytes)
	}
}

func TestBuildEventsURLPreservesSpecialCursorChars(t *testing.T) {
	cursor := "cursor:1&evil=true#frag+plus space%20"
	built, err := buildEventsURL("http://127.0.0.1:8787/events", cursor)
	if err != nil {
		t.Fatalf("buildEventsURL failed: %v", err)
	}
	parsed, err := url.Parse(built)
	if err != nil {
		t.Fatalf("url.Parse(%q) failed: %v", built, err)
	}
	if parsed.Fragment != "" {
		t.Fatalf("expected empty fragment, got %q", parsed.Fragment)
	}
	q := parsed.Query()
	if got := q.Get("after"); got != cursor {
		t.Fatalf("query after=%q, want %q", got, cursor)
	}
	if q.Has("evil") {
		t.Fatalf("cursor injected extra query parameter evil=%q", q.Get("evil"))
	}
}

func TestRunSmokeStopsRunawayPagination(t *testing.T) {
	vectorsPath := filepath.Join("..", "..", "..", "conformance", "vectors", "wire-vectors.json")
	vectors, err := loadVectors(vectorsPath)
	if err != nil {
		t.Fatalf("loadVectors: %v", err)
	}

	var (
		mu         sync.Mutex
		byID       = make(map[string]mockEvent)
		nextSeq    int64
		eventPages atomic.Int32
	)

	writeProblem := func(w http.ResponseWriter, status int, code string) {
		w.Header().Set("Content-Type", "application/problem+json")
		w.Header().Set("Cache-Control", "no-store")
		w.WriteHeader(status)
		_ = json.NewEncoder(w).Encode(map[string]any{
			"type":   "https://agentciv.io/problems/" + code,
			"title":  code,
			"status": status,
			"code":   code,
		})
	}

	var server *httptest.Server
	server = httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		switch {
		case r.Method == http.MethodGet && r.URL.Path == "/.well-known/agentciv":
			w.Header().Set("Content-Type", "application/json")
			_ = json.NewEncoder(w).Encode(map[string]any{
				"protocol_version": "0.1-draft",
				"profile":          vectors.Profile,
				"id":               vectors.World,
				"endpoints": map[string]string{
					"events": server.URL + "/events",
					"submit": server.URL + "/submit",
				},
			})
		case r.Method == http.MethodPost && r.URL.Path == "/submit":
			raw, err := io.ReadAll(r.Body)
			if err != nil || validateUniqueJSON(raw) != nil {
				writeProblem(w, http.StatusBadRequest, "malformed_json")
				return
			}
			var rec map[string]any
			if err := json.Unmarshal(raw, &rec); err != nil {
				writeProblem(w, http.StatusBadRequest, "malformed_json")
				return
			}
			recordID, _ := rec["id"].(string)
			mu.Lock()
			defer mu.Unlock()
			if existing, ok := byID[recordID]; ok {
				if !bytes.Equal(existing.Raw, raw) {
					writeProblem(w, http.StatusConflict, "id_conflict")
					return
				}
				w.Header().Set("Content-Type", "application/json")
				w.Header().Set("Cache-Control", "no-store")
				_ = json.NewEncoder(w).Encode(map[string]any{
					"status":    "recorded",
					"event_id":  existing.ID,
					"record_id": recordID,
					"sequence":  existing.Sequence,
				})
				return
			}
			nextSeq++
			ev := mockEvent{
				ID:       fmt.Sprintf("event:%d", nextSeq),
				Sequence: nextSeq,
				Raw:      append([]byte(nil), raw...),
				Record:   rec,
			}
			byID[recordID] = ev
			w.Header().Set("Content-Type", "application/json")
			w.Header().Set("Cache-Control", "no-store")
			_ = json.NewEncoder(w).Encode(map[string]any{
				"status":    "recorded",
				"event_id":  ev.ID,
				"record_id": recordID,
				"sequence":  ev.Sequence,
			})
		case r.Method == http.MethodGet && r.URL.Path == "/events":
			n := eventPages.Add(1)
			w.Header().Set("Content-Type", "application/json")
			w.Header().Set("Cache-Control", "no-store")
			_ = json.NewEncoder(w).Encode(map[string]any{
				"events":      []any{},
				"has_more":    true,
				"next_cursor": fmt.Sprintf("cursor:%d", n),
			})
		default:
			writeProblem(w, http.StatusNotFound, "not_found")
		}
	}))
	defer server.Close()

	cfg := smokeConfig{
		Origin:      server.URL,
		WriterToken: "token-writer",
		ReaderToken: "token-reader",
		VectorsPath: vectorsPath,
	}
	err = runSmoke(cfg, server.Client())
	if err == nil {
		t.Fatalf("runSmoke succeeded on runaway pagination, want error")
	}
	if !strings.Contains(err.Error(), "exceeded") {
		t.Fatalf("expected pagination limit error, got: %v", err)
	}
	if got := int(eventPages.Load()); got != maxEventPages {
		t.Fatalf("fetched %d pages before stopping, want %d", got, maxEventPages)
	}
}
