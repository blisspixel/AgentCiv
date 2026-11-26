// Command go-smoke exercises http-commons/0.1-draft with Go's standard library.
// It uses SetEscapeHTML(false) to preserve HTML-significant bytes and runs the
// frozen language-neutral wire vectors against a loopback host.
package main

import (
	"bytes"
	"encoding/json"
	"flag"
	"fmt"
	"io"
	"net/http"
	"os"
	"strings"
	"time"
)

type wireVectors struct {
	Format    string           `json:"format"`
	Profile   string           `json:"profile"`
	World     string           `json:"world"`
	Principal string           `json:"principal"`
	Recipient string           `json:"recipient"`
	Positive  []positiveVector `json:"positive"`
	Negative  []negativeVector `json:"negative"`
}

type positiveVector struct {
	ID                       string         `json:"id"`
	Category                 string         `json:"category"`
	Description              string         `json:"description"`
	CheckEscapedConflict     bool           `json:"check_escaped_conflict"`
	ForbiddenResponseHeaders []string       `json:"forbidden_response_headers"`
	Record                   map[string]any `json:"record"`
	ExpectedBodyText         string         `json:"expected_body_text"`
}

type negativeVector struct {
	ID             string `json:"id"`
	Category       string `json:"category"`
	Description    string `json:"description"`
	Endpoint       string `json:"endpoint"`
	RawJSON        string `json:"raw_json"`
	ExpectedStatus int    `json:"expected_status"`
	ExpectedCode   string `json:"expected_code"`
}

type worldDescriptor struct {
	ProtocolVersion string `json:"protocol_version"`
	Profile         string `json:"profile"`
	ID              string `json:"id"`
	Endpoints       struct {
		Events      string `json:"events"`
		Submit      string `json:"submit"`
		Collaborate string `json:"collaborate"`
	} `json:"endpoints"`
}

type submitReceipt struct {
	Status   string `json:"status"`
	EventID  string `json:"event_id"`
	RecordID string `json:"record_id"`
	Sequence int64  `json:"sequence"`
}

type problemResponse struct {
	Status int    `json:"status"`
	Code   string `json:"code"`
	Title  string `json:"title"`
}

type eventPage struct {
	Events     []eventEnvelope `json:"events"`
	HasMore    bool            `json:"has_more"`
	NextCursor string          `json:"next_cursor"`
}

type eventEnvelope struct {
	ID       string `json:"id"`
	Kind     string `json:"kind"`
	Sequence int64  `json:"sequence"`
	Body     struct {
		Message *struct {
			ID    string `json:"id"`
			World string `json:"world"`
			From  string `json:"from"`
			Body  struct {
				Text string `json:"text"`
			} `json:"body"`
		} `json:"message"`
	} `json:"body"`
}

type smokeConfig struct {
	Origin      string
	WriterToken string
	ReaderToken string
	VectorsPath string
}

// encodeNoEscapeHTML serializes v as UTF-8 JSON without Go's default HTML escaping
// of '<', '>', and '&', preserving exact wire bytes for idempotent retries.
func encodeNoEscapeHTML(v any) ([]byte, error) {
	var buf bytes.Buffer
	enc := json.NewEncoder(&buf)
	enc.SetEscapeHTML(false)
	if err := enc.Encode(v); err != nil {
		return nil, err
	}
	return bytes.TrimSuffix(buf.Bytes(), []byte("\n")), nil
}

func loadVectors(path string) (*wireVectors, error) {
	raw, err := os.ReadFile(path)
	if err != nil {
		return nil, fmt.Errorf("read vectors %s: %w", path, err)
	}
	var vectors wireVectors
	if err := json.Unmarshal(raw, &vectors); err != nil {
		return nil, fmt.Errorf("parse vectors %s: %w", path, err)
	}
	if vectors.Format != "agentciv-wire-vectors/0.1" {
		return nil, fmt.Errorf("unexpected vector format: %q", vectors.Format)
	}
	if len(vectors.Positive) == 0 || len(vectors.Negative) == 0 {
		return nil, fmt.Errorf("vectors file must contain positive and negative cases")
	}
	return &vectors, nil
}

func doRequest(client *http.Client, method, url, token string, body []byte) (int, http.Header, []byte, error) {
	var reader io.Reader
	if body != nil {
		reader = bytes.NewReader(body)
	}
	req, err := http.NewRequest(method, url, reader)
	if err != nil {
		return 0, nil, nil, err
	}
	req.Header.Set("Accept", "application/json")
	if token != "" {
		req.Header.Set("Authorization", "Bearer "+token)
	}
	if body != nil {
		req.Header.Set("Content-Type", "application/json")
	}
	resp, err := client.Do(req)
	if err != nil {
		return 0, nil, nil, err
	}
	defer resp.Body.Close()
	data, err := io.ReadAll(io.LimitReader(resp.Body, 2*1024*1024))
	if err != nil {
		return resp.StatusCode, resp.Header, nil, err
	}
	return resp.StatusCode, resp.Header, data, nil
}

func runSmoke(cfg smokeConfig, client *http.Client) error {
	vectors, err := loadVectors(cfg.VectorsPath)
	if err != nil {
		return err
	}
	origin := strings.TrimRight(cfg.Origin, "/")

	// 1. Public discovery.
	status, headers, body, err := doRequest(client, http.MethodGet, origin+"/.well-known/agentciv", "", nil)
	if err != nil {
		return fmt.Errorf("discovery request failed: %w", err)
	}
	if status != http.StatusOK {
		return fmt.Errorf("discovery returned HTTP %d: %s", status, string(body))
	}
	if !strings.HasPrefix(strings.ToLower(headers.Get("Content-Type")), "application/json") {
		return fmt.Errorf("discovery content-type was %q", headers.Get("Content-Type"))
	}
	var world worldDescriptor
	if err := json.Unmarshal(body, &world); err != nil {
		return fmt.Errorf("decode discovery: %w", err)
	}
	if world.Profile != vectors.Profile || world.ID != vectors.World {
		return fmt.Errorf("discovery mismatch: profile=%q id=%q", world.Profile, world.ID)
	}
	if world.Endpoints.Submit == "" || world.Endpoints.Events == "" {
		return fmt.Errorf("discovery omitted submit or events endpoint")
	}

	expectedText := make(map[string]string, len(vectors.Positive))
	recorded := make(map[string]submitReceipt, len(vectors.Positive))

	// 2. Positive vectors: submit, exact-byte retry, and escaped-HTML conflict check.
	for _, vec := range vectors.Positive {
		raw, err := encodeNoEscapeHTML(vec.Record)
		if err != nil {
			return fmt.Errorf("encode vector %s: %w", vec.ID, err)
		}
		recordID, _ := vec.Record["id"].(string)

		status, headers, body, err := doRequest(client, http.MethodPost, world.Endpoints.Submit, cfg.WriterToken, raw)
		if err != nil {
			return fmt.Errorf("submit %s: %w", vec.ID, err)
		}
		if status != http.StatusOK {
			return fmt.Errorf("submit %s returned HTTP %d: %s", vec.ID, status, string(body))
		}
		if headers.Get("Cache-Control") != "no-store" {
			return fmt.Errorf("submit %s Cache-Control was %q", vec.ID, headers.Get("Cache-Control"))
		}
		for _, forbidden := range vec.ForbiddenResponseHeaders {
			if headers.Get(forbidden) != "" {
				return fmt.Errorf("submit %s injected forbidden header %s", vec.ID, forbidden)
			}
		}
		var receipt submitReceipt
		if err := json.Unmarshal(body, &receipt); err != nil {
			return fmt.Errorf("decode receipt %s: %w", vec.ID, err)
		}
		if receipt.Status != "recorded" || receipt.RecordID != recordID {
			return fmt.Errorf("unexpected receipt for %s: %+v", vec.ID, receipt)
		}

		// Exact-byte idempotent retry.
		retryStatus, _, retryBody, err := doRequest(client, http.MethodPost, world.Endpoints.Submit, cfg.WriterToken, raw)
		if err != nil {
			return fmt.Errorf("retry %s: %w", vec.ID, err)
		}
		if retryStatus != http.StatusOK {
			return fmt.Errorf("retry %s returned HTTP %d: %s", vec.ID, retryStatus, string(retryBody))
		}
		var retryReceipt submitReceipt
		if err := json.Unmarshal(retryBody, &retryReceipt); err != nil {
			return fmt.Errorf("decode retry receipt %s: %w", vec.ID, err)
		}
		if retryReceipt.EventID != receipt.EventID || retryReceipt.Sequence != receipt.Sequence {
			return fmt.Errorf("retry %s minted new event: first=%+v retry=%+v", vec.ID, receipt, retryReceipt)
		}

		// Verify Go's default json.Marshal (which escapes <, >, &) triggers 409 id_conflict.
		if vec.CheckEscapedConflict {
			escaped, err := json.Marshal(vec.Record)
			if err != nil {
				return fmt.Errorf("marshal escaped %s: %w", vec.ID, err)
			}
			if bytes.Equal(escaped, raw) {
				return fmt.Errorf("vector %s expected json.Marshal to differ from SetEscapeHTML(false)", vec.ID)
			}
			conflictStatus, _, conflictBody, err := doRequest(client, http.MethodPost, world.Endpoints.Submit, cfg.WriterToken, escaped)
			if err != nil {
				return fmt.Errorf("conflict %s: %w", vec.ID, err)
			}
			if conflictStatus != http.StatusConflict {
				return fmt.Errorf("escaped retry %s returned HTTP %d, want 409: %s", vec.ID, conflictStatus, string(conflictBody))
			}
			var prob problemResponse
			if err := json.Unmarshal(conflictBody, &prob); err != nil {
				return fmt.Errorf("decode conflict problem %s: %w", vec.ID, err)
			}
			if prob.Code != "id_conflict" {
				return fmt.Errorf("escaped retry %s code=%q, want id_conflict", vec.ID, prob.Code)
			}
		}

		expectedText[recordID] = vec.ExpectedBodyText
		recorded[recordID] = receipt
	}

	// 3. Negative vectors: verify strict rejection.
	for _, vec := range vectors.Negative {
		target := world.Endpoints.Submit
		if vec.Endpoint == "/collaborate" {
			if world.Endpoints.Collaborate == "" {
				continue
			}
			target = world.Endpoints.Collaborate
		}
		status, headers, body, err := doRequest(client, http.MethodPost, target, cfg.WriterToken, []byte(vec.RawJSON))
		if err != nil {
			return fmt.Errorf("negative %s: %w", vec.ID, err)
		}
		if status != vec.ExpectedStatus {
			return fmt.Errorf("negative %s returned HTTP %d, want %d: %s", vec.ID, status, vec.ExpectedStatus, string(body))
		}
		if !strings.Contains(strings.ToLower(headers.Get("Content-Type")), "application/problem+json") {
			return fmt.Errorf("negative %s content-type=%q, want application/problem+json", vec.ID, headers.Get("Content-Type"))
		}
		var prob problemResponse
		if err := json.Unmarshal(body, &prob); err != nil {
			return fmt.Errorf("decode negative problem %s: %w", vec.ID, err)
		}
		if prob.Code != vec.ExpectedCode {
			return fmt.Errorf("negative %s code=%q, want %q", vec.ID, prob.Code, vec.ExpectedCode)
		}
	}

	// 4. Read back events and verify strictly opaque round-trip.
	var allEvents []eventEnvelope
	cursor := ""
	for {
		url := world.Endpoints.Events
		if cursor != "" {
			url += "?after=" + cursor
		}
		status, headers, body, err := doRequest(client, http.MethodGet, url, cfg.ReaderToken, nil)
		if err != nil {
			return fmt.Errorf("read events: %w", err)
		}
		if status != http.StatusOK {
			return fmt.Errorf("read events returned HTTP %d: %s", status, string(body))
		}
		if headers.Get("X-Injected-Header") != "" || headers.Get("Set-Cookie") != "" {
			return fmt.Errorf("events response contained injected header")
		}
		var page eventPage
		if err := json.Unmarshal(body, &page); err != nil {
			return fmt.Errorf("decode events page: %w", err)
		}
		allEvents = append(allEvents, page.Events...)
		if !page.HasMore || page.NextCursor == "" || page.NextCursor == cursor {
			break
		}
		cursor = page.NextCursor
	}

	if len(allEvents) != len(expectedText) {
		return fmt.Errorf("read %d events, want %d", len(allEvents), len(expectedText))
	}
	for _, ev := range allEvents {
		if ev.Kind != "message.recorded" || ev.Body.Message == nil {
			return fmt.Errorf("unexpected event kind %q", ev.Kind)
		}
		msg := ev.Body.Message
		wantText, ok := expectedText[msg.ID]
		if !ok {
			return fmt.Errorf("unexpected message id %q in events", msg.ID)
		}
		if msg.From != vectors.Principal || msg.World != vectors.World {
			return fmt.Errorf("host metadata altered for %s: from=%q world=%q", msg.ID, msg.From, msg.World)
		}
		if msg.Body.Text != wantText {
			return fmt.Errorf("opaque text mismatch for %s: got %q, want %q", msg.ID, msg.Body.Text, wantText)
		}
		rec := recorded[msg.ID]
		if ev.ID != rec.EventID || ev.Sequence != rec.Sequence {
			return fmt.Errorf("event receipt mismatch for %s: event=(%s,%d) receipt=(%s,%d)", msg.ID, ev.ID, ev.Sequence, rec.EventID, rec.Sequence)
		}
	}

	return nil
}

func main() {
	origin := flag.String("origin", "", "Loopback host origin (for example http://127.0.0.1:8787)")
	writerToken := flag.String("writer-token", "", "Bearer token for the writer principal")
	readerToken := flag.String("reader-token", "", "Bearer token for the reader principal")
	vectorsPath := flag.String("vectors", "../../../conformance/vectors/wire-vectors.json", "Path to wire-vectors.json")
	flag.Parse()

	if *origin == "" || *writerToken == "" || *readerToken == "" {
		fmt.Fprintln(os.Stderr, "Usage: go-smoke --origin URL --writer-token TOKEN --reader-token TOKEN [--vectors PATH]")
		os.Exit(2)
	}

	client := &http.Client{Timeout: 10 * time.Second}
	cfg := smokeConfig{
		Origin:      *origin,
		WriterToken: *writerToken,
		ReaderToken: *readerToken,
		VectorsPath: *vectorsPath,
	}
	if err := runSmoke(cfg, client); err != nil {
		fmt.Fprintf(os.Stderr, "go-smoke failed: %v\n", err)
		os.Exit(1)
	}
	fmt.Println("go-smoke passed all wire vectors")
}
