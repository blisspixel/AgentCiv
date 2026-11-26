package main

import (
	"bytes"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"net/http/httptest"
	"path/filepath"
	"strings"
	"sync"
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
