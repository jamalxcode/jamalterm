package ai

import (
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"net/http/httptest"
	"strings"
	"sync"
	"testing"
)

// fakeAPI answers like the Messages API's streaming endpoint.
type fakeAPI struct {
	mu     sync.Mutex
	bodies []map[string]any
	header []http.Header
	status int
	reply  []string
}

func (f *fakeAPI) ServeHTTP(w http.ResponseWriter, r *http.Request) {
	body, _ := io.ReadAll(r.Body)
	var m map[string]any
	json.Unmarshal(body, &m)
	f.mu.Lock()
	f.bodies = append(f.bodies, m)
	f.header = append(f.header, r.Header.Clone())
	status, reply := f.status, f.reply
	f.mu.Unlock()

	if status != 0 {
		w.Header().Set("Content-Type", "application/json")
		w.WriteHeader(status)
		fmt.Fprint(w, `{"type":"error","error":{"type":"authentication_error","message":"invalid x-api-key"}}`)
		return
	}
	w.Header().Set("Content-Type", "text/event-stream")
	send := func(event, data string) { fmt.Fprintf(w, "event: %s\ndata: %s\n\n", event, data) }
	send("message_start", `{"type":"message_start","message":{"id":"msg_1","type":"message","role":"assistant","model":"claude-opus-5-5","content":[],"stop_reason":null,"usage":{"input_tokens":10,"output_tokens":0}}}`)
	send("content_block_start", `{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}`)
	for _, part := range reply {
		b, _ := json.Marshal(part)
		send("content_block_delta", `{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":`+string(b)+`}}`)
	}
	send("content_block_stop", `{"type":"content_block_stop","index":0}`)
	send("message_delta", `{"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":5}}`)
	send("message_stop", `{"type":"message_stop"}`)
}

func setup(t *testing.T, f *fakeAPI, model string) *Client {
	srv := httptest.NewServer(f)
	t.Cleanup(srv.Close)
	t.Setenv("JAMALTERM_ANTHROPIC_BASE_URL", srv.URL)
	return New(func() string { return "sk-test" }, func() string { return model })
}

func TestChatStreamsAndKeepsHistory(t *testing.T) {
	f := &fakeAPI{reply: []string{"Use ", "`ls -la`", "."}}
	c := setup(t, f, "claude-opus-5-5")

	var deltas []string
	got, err := c.Ask(context.Background(), Request{ID: "1", Chat: "tab1", Mode: ModeChat, Prompt: "list files", Context: "$ pwd\n/Users/j", Shell: "zsh"},
		func(s string) { deltas = append(deltas, s) })
	if err != nil {
		t.Fatal(err)
	}
	if got != "Use `ls -la`." || strings.Join(deltas, "") != got || len(deltas) != 3 {
		t.Fatalf("answer %q deltas %q", got, deltas)
	}

	body, h := f.bodies[0], f.header[0]
	if body["model"] != "claude-opus-5-5" || body["fallbacks"] != "default" || body["stream"] != true {
		t.Fatalf("body: %v", body)
	}
	if h.Get("Authorization") != "Bearer sk-test" || !strings.Contains(h.Get("Anthropic-Beta"), "server-side-fallback-2026-07-01") {
		t.Fatalf("headers: %v", h)
	}
	if oc, _ := body["output_config"].(map[string]any); oc["effort"] != "medium" {
		t.Fatalf("effort: %v", body["output_config"])
	}
	msgs := body["messages"].([]any)
	first := msgs[0].(map[string]any)["content"].([]any)[0].(map[string]any)["text"].(string)
	if !strings.Contains(first, "<terminal>\n$ pwd\n/Users/j\n</terminal>") || !strings.HasSuffix(first, "list files") {
		t.Fatalf("user message: %q", first)
	}

	// Second question in the same chat sends the earlier turns back.
	f.reply = []string{"ok"}
	if _, err := c.Ask(context.Background(), Request{ID: "2", Chat: "tab1", Mode: ModeChat, Prompt: "and hidden ones?"}, func(string) {}); err != nil {
		t.Fatal(err)
	}
	if n := len(f.bodies[1]["messages"].([]any)); n != 3 {
		t.Fatalf("second request has %d messages, want 3", n)
	}
	// Another chat starts fresh; Reset forgets.
	c.Ask(context.Background(), Request{ID: "3", Chat: "tab2", Mode: ModeChat, Prompt: "hi"}, func(string) {})
	if n := len(f.bodies[2]["messages"].([]any)); n != 1 {
		t.Fatalf("other chat has %d messages", n)
	}
	c.Reset("tab1")
	c.Ask(context.Background(), Request{ID: "4", Chat: "tab1", Mode: ModeChat, Prompt: "hi"}, func(string) {})
	if n := len(f.bodies[3]["messages"].([]any)); n != 1 {
		t.Fatalf("after reset %d messages", n)
	}
}

func TestCommandModeIsOneShotAndLowEffort(t *testing.T) {
	f := &fakeAPI{reply: []string{"find ~/Downloads -size +100M"}}
	c := setup(t, f, "claude-haiku-5-5")
	for i := 0; i < 2; i++ {
		got, err := c.Ask(context.Background(), Request{ID: "x", Chat: "cmd", Mode: ModeCommand, Prompt: "big files in downloads"}, func(string) {})
		if err != nil || got != "find ~/Downloads -size +100M" {
			t.Fatal(got, err)
		}
	}
	b := f.bodies[1]
	if n := len(b["messages"].([]any)); n != 1 {
		t.Fatalf("command mode kept history: %d", n)
	}
	if oc, _ := b["output_config"].(map[string]any); oc["effort"] != "low" {
		t.Fatalf("effort: %v", b["output_config"])
	}
	if _, has := b["fallbacks"]; has {
		t.Fatal("haiku request must not ask for fallbacks")
	}
}

func TestErrors(t *testing.T) {
	c := New(func() string { return "" }, func() string { return "claude-opus-5-5" })
	if _, err := c.Ask(context.Background(), Request{Prompt: "x"}, func(string) {}); err != ErrNoKey {
		t.Fatalf("no key: %v", err)
	}
	f := &fakeAPI{status: 401}
	c = setup(t, f, "claude-opus-5-5")
	_, err := c.Ask(context.Background(), Request{ID: "1", Prompt: "x"}, func(string) {})
	if err == nil || !strings.Contains(err.Error(), "API key was rejected") {
		t.Fatalf("401: %v", err)
	}
}
