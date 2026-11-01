package ai

import (
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"testing"
	"time"
)

// fakeAPI answers like the Messages API's streaming endpoint.
type fakeAPI struct {
	mu     sync.Mutex
	bodies []map[string]any
	header []http.Header
	status int
	reply  []string
	// toolInputs: raw tool inputs answered (one per request) before the text reply.
	toolInputs []string
}

func (f *fakeAPI) ServeHTTP(w http.ResponseWriter, r *http.Request) {
	body, _ := io.ReadAll(r.Body)
	var m map[string]any
	json.Unmarshal(body, &m)
	f.mu.Lock()
	f.bodies = append(f.bodies, m)
	f.header = append(f.header, r.Header.Clone())
	status, reply := f.status, f.reply
	var toolInput string
	if len(f.toolInputs) > 0 {
		toolInput, f.toolInputs = f.toolInputs[0], f.toolInputs[1:]
	}
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
	if toolInput != "" {
		send("content_block_start", `{"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"toolu_1","name":"type_in_terminal","input":{}}}`)
		b, _ := json.Marshal(toolInput)
		send("content_block_delta", `{"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":`+string(b)+`}}`)
		send("content_block_stop", `{"type":"content_block_stop","index":0}`)
		send("message_delta", `{"type":"message_delta","delta":{"stop_reason":"tool_use"},"usage":{"output_tokens":5}}`)
		send("message_stop", `{"type":"message_stop"}`)
		return
	}
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
		func(s string) { deltas = append(deltas, s) }, nil)
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
	if _, err := c.Ask(context.Background(), Request{ID: "2", Chat: "tab1", Mode: ModeChat, Prompt: "and hidden ones?"}, func(string) {}, nil); err != nil {
		t.Fatal(err)
	}
	if n := len(f.bodies[1]["messages"].([]any)); n != 3 {
		t.Fatalf("second request has %d messages, want 3", n)
	}
	// Another chat starts fresh; Reset forgets.
	c.Ask(context.Background(), Request{ID: "3", Chat: "tab2", Mode: ModeChat, Prompt: "hi"}, func(string) {}, nil)
	if n := len(f.bodies[2]["messages"].([]any)); n != 1 {
		t.Fatalf("other chat has %d messages", n)
	}
	c.Reset("tab1")
	c.Ask(context.Background(), Request{ID: "4", Chat: "tab1", Mode: ModeChat, Prompt: "hi"}, func(string) {}, nil)
	if n := len(f.bodies[3]["messages"].([]any)); n != 1 {
		t.Fatalf("after reset %d messages", n)
	}
}

func TestCommandModeIsOneShotAndLowEffort(t *testing.T) {
	f := &fakeAPI{reply: []string{"find ~/Downloads -size +100M"}}
	c := setup(t, f, "claude-haiku-5-5")
	for i := 0; i < 2; i++ {
		got, err := c.Ask(context.Background(), Request{ID: "x", Chat: "cmd", Mode: ModeCommand, Prompt: "big files in downloads"}, func(string) {}, nil)
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
	t.Setenv("ANTHROPIC_CONFIG_DIR", t.TempDir()) // not signed in
	c := New(func() string { return "" }, func() string { return "claude-opus-5-5" })
	if _, err := c.Ask(context.Background(), Request{Prompt: "x"}, func(string) {}, nil); err != ErrNoKey {
		t.Fatalf("no key: %v", err)
	}
	f := &fakeAPI{status: 401}
	c = setup(t, f, "claude-opus-5-5")
	_, err := c.Ask(context.Background(), Request{ID: "1", Prompt: "x"}, func(string) {}, nil)
	if err == nil || !strings.Contains(err.Error(), "didn't accept your API key") {
		t.Fatalf("401: %v", err)
	}
}

// writeProfile fakes what `ant auth login` leaves on disk.
func writeProfile(t *testing.T, token string) {
	dir := t.TempDir()
	t.Setenv("ANTHROPIC_CONFIG_DIR", dir)
	os.MkdirAll(filepath.Join(dir, "configs"), 0o700)
	os.MkdirAll(filepath.Join(dir, "credentials"), 0o700)
	os.WriteFile(filepath.Join(dir, "configs", "default.json"), []byte(`{"authentication":{"type":"user_oauth"},"workspace_id":"wrkspc_test"}`), 0o600)
	os.WriteFile(filepath.Join(dir, "credentials", "default.json"),
		[]byte(fmt.Sprintf(`{"type":"oauth_token","access_token":%q,"expires_at":%d}`, token, time.Now().Add(time.Hour).Unix())), 0o600)
}

func TestSignInWithAnthropic(t *testing.T) {
	t.Setenv("ANTHROPIC_API_KEY", "")
	os.Unsetenv("ANTHROPIC_API_KEY")
	os.Unsetenv("ANTHROPIC_AUTH_TOKEN")
	os.Unsetenv("ANTHROPIC_PROFILE")

	// Not signed in, no key: a clear message, no request.
	t.Setenv("ANTHROPIC_CONFIG_DIR", t.TempDir())
	if SignedInAccount().SignedIn {
		t.Fatal("empty config dir reported as signed in")
	}
	c := New(func() string { return "" }, func() string { return "claude-opus-5-5" })
	if _, err := c.Ask(context.Background(), Request{Prompt: "x"}, func(string) {}, nil); err != ErrNoKey {
		t.Fatalf("want ErrNoKey, got %v", err)
	}

	// Signed in: the request carries the OAuth token, the oauth beta
	// header and the workspace, and no API key.
	writeProfile(t, "oauth-tok-123")
	acct := SignedInAccount()
	if !acct.SignedIn || acct.Profile != "default" || acct.Workspace != "wrkspc_test" {
		t.Fatalf("account: %+v", acct)
	}
	f := &fakeAPI{reply: []string{"hi"}}
	srv := httptest.NewServer(f)
	defer srv.Close()
	t.Setenv("JAMALTERM_ANTHROPIC_BASE_URL", srv.URL)
	if _, err := c.Ask(context.Background(), Request{ID: "1", Chat: "c", Mode: ModeChat, Prompt: "hi"}, func(string) {}, nil); err != nil {
		t.Fatal(err)
	}
	h := f.header[0]
	if h.Get("Authorization") != "Bearer oauth-tok-123" {
		t.Fatalf("Authorization = %q", h.Get("Authorization"))
	}
	if !strings.Contains(h.Get("Anthropic-Beta"), "oauth-2025-04-20") || !strings.Contains(h.Get("Anthropic-Beta"), "server-side-fallback") {
		t.Fatalf("anthropic-beta = %q", h.Get("Anthropic-Beta"))
	}
	if h.Get("X-Api-Key") != "" {
		t.Fatal("sent an API key")
	}

	// A saved API key still wins over the sign-in.
	c2 := New(func() string { return "sk-saved" }, func() string { return "claude-opus-5-5" })
	c2.Ask(context.Background(), Request{ID: "2", Chat: "c", Mode: ModeChat, Prompt: "hi"}, func(string) {}, nil)
	if got := f.header[1].Get("Authorization"); got != "Bearer sk-saved" {
		t.Fatalf("key should win, got %q", got)
	}
}

func TestTypeInTerminalTool(t *testing.T) {
	f := &fakeAPI{
		toolInputs: []string{`{"command":"say -v Zarvox \"Hello world\"","run":true}`, `{"command":"","run":false}`},
		reply:      []string{"Done, it said hello."},
	}
	c := setup(t, f, "claude-opus-5-5")
	var calls []ToolCall
	got, err := c.Ask(context.Background(), Request{ID: "1", Chat: "t", Mode: ModeChat, Prompt: "say hello in a robot voice"},
		func(string) {},
		func(tc ToolCall) (string, bool) {
			calls = append(calls, tc)
			return "exit 0", false
		})
	if err != nil {
		t.Fatal(err)
	}
	// The valid call ran once; the empty command never reached the handler.
	if len(calls) != 1 || calls[0].Command != `say -v Zarvox "Hello world"` || !calls[0].Run || calls[0].ID != "toolu_1" {
		t.Fatalf("calls: %+v", calls)
	}
	if got != "Done, it said hello." {
		t.Fatalf("answer %q", got)
	}
	// Request 1 offers the tool; 2 returns the result; 3 returns an INVALID_JSON error.
	if len(f.bodies) != 3 {
		t.Fatalf("%d requests", len(f.bodies))
	}
	tools := f.bodies[0]["tools"].([]any)
	tool := tools[0].(map[string]any)
	if tool["name"] != "type_in_terminal" || tool["eager_input_streaming"] != true {
		t.Fatalf("tool def: %v", tool)
	}
	lastOf := func(i int) map[string]any {
		msgs := f.bodies[i]["messages"].([]any)
		return msgs[len(msgs)-1].(map[string]any)["content"].([]any)[0].(map[string]any)
	}
	if r := lastOf(1); r["type"] != "tool_result" || r["tool_use_id"] != "toolu_1" || r["is_error"] == true {
		t.Fatalf("result 1: %v", r)
	}
	if r := lastOf(2); r["is_error"] != true || !strings.Contains(fmt.Sprint(r["content"]), "INVALID_JSON") {
		t.Fatalf("result 2: %v", r)
	}
	// The whole exchange is kept for the next question.
	f.reply = []string{"ok"}
	c.Ask(context.Background(), Request{ID: "2", Chat: "t", Mode: ModeChat, Prompt: "again"}, func(string) {}, nil)
	if n := len(f.bodies[3]["messages"].([]any)); n != 7 {
		t.Fatalf("history has %d messages, want 7", n)
	}
	// Command mode never offers tools.
	c.Ask(context.Background(), Request{ID: "3", Chat: "x", Mode: ModeCommand, Prompt: "x"}, func(string) {}, func(ToolCall) (string, bool) { return "", false })
	if _, has := f.bodies[4]["tools"]; has {
		t.Fatal("command mode sent tools")
	}
}
