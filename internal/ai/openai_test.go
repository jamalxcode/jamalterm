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

// fakeCompat answers like the OpenAI (and DeepSeek) chat completions API.
type fakeCompat struct {
	mu        sync.Mutex
	bodies    []map[string]any
	auth      []string
	toolInput string // first reply calls the tool with this, if set
	reasoning string // DeepSeek-style reasoning_content in the stream
	status    int
}

func (f *fakeCompat) ServeHTTP(w http.ResponseWriter, r *http.Request) {
	if f.status != 0 {
		w.Header().Set("Content-Type", "application/json")
		w.WriteHeader(f.status)
		fmt.Fprint(w, `{"error":{"message":"bad key","type":"invalid_request_error"}}`)
		return
	}
	if r.Method == "GET" && strings.HasSuffix(r.URL.Path, "/models") {
		w.Header().Set("Content-Type", "application/json")
		fmt.Fprint(w, `{"object":"list","data":[
		  {"id":"gpt-4o-mini","object":"model"},{"id":"gpt-6.1-sol","object":"model"},
		  {"id":"gpt-6-luna","object":"model"},{"id":"gpt-image-2","object":"model"},
		  {"id":"text-embedding-3-large","object":"model"},{"id":"gpt-6.1-sol-2026-09-01","object":"model"}]}`)
		return
	}
	body, _ := io.ReadAll(r.Body)
	var m map[string]any
	json.Unmarshal(body, &m)
	f.mu.Lock()
	f.bodies = append(f.bodies, m)
	f.auth = append(f.auth, r.Header.Get("Authorization"))
	first := len(f.bodies) == 1
	f.mu.Unlock()

	w.Header().Set("Content-Type", "text/event-stream")
	send := func(delta string, finish string) {
		fin := "null"
		if finish != "" {
			fin = `"` + finish + `"`
		}
		fmt.Fprintf(w, "data: {\"id\":\"c1\",\"object\":\"chat.completion.chunk\",\"created\":1,\"model\":\"m\",\"choices\":[{\"index\":0,\"delta\":%s,\"finish_reason\":%s}]}\n\n", delta, fin)
	}
	if f.reasoning != "" {
		b, _ := json.Marshal(f.reasoning)
		send(`{"role":"assistant","reasoning_content":`+string(b)+`}`, "")
	}
	if first && f.toolInput != "" {
		args, _ := json.Marshal(f.toolInput)
		send(`{"role":"assistant","content":"On it."}`, "")
		send(`{"tool_calls":[{"index":0,"id":"call_1","type":"function","function":{"name":"type_in_terminal","arguments":`+string(args)+`}}]}`, "")
		send(`{}`, "tool_calls")
	} else {
		send(`{"role":"assistant","content":"Hello "}`, "")
		send(`{"content":"there."}`, "")
		send(`{}`, "stop")
	}
	fmt.Fprint(w, "data: [DONE]\n\n")
}

func compatSetup(t *testing.T, f *fakeCompat, provider, model string) *Client {
	srv := httptest.NewServer(f)
	t.Cleanup(srv.Close)
	t.Setenv("JAMALTERM_OPENAI_BASE_URL", srv.URL+"/v1")
	t.Setenv("JAMALTERM_DEEPSEEK_BASE_URL", srv.URL)
	c := New(func() string { return "" }, func() string { return "claude-opus-5-5" })
	c.Configure(func() string { return provider },
		func(p string) string { return "key-" + p },
		func(string) string { return model })
	return c
}

func TestChatGPTAutoModelAndTool(t *testing.T) {
	f := &fakeCompat{toolInput: `{"command":"say -v Zarvox hi","run":false}`}
	c := compatSetup(t, f, ProviderOpenAI, "") // auto model
	var deltas strings.Builder
	var calls []ToolCall
	got, err := c.Ask(context.Background(), Request{ID: "1", Chat: "tab1", Mode: ModeChat, Prompt: "paste a robot hello", Shell: "zsh"},
		func(s string) { deltas.WriteString(s) },
		func(tc ToolCall) (string, bool) { calls = append(calls, tc); return "typed", false })
	if err != nil {
		t.Fatal(err)
	}
	if got != "On it.\n\nHello there." || deltas.String() != got {
		t.Fatalf("answer %q deltas %q", got, deltas.String())
	}
	if len(calls) != 1 || calls[0].Command != "say -v Zarvox hi" || calls[0].Run || calls[0].ID != "call_1" {
		t.Fatalf("calls %+v", calls)
	}
	b0, b1 := f.bodies[0], f.bodies[1]
	// Auto picks the newest general model, not mini/luna/dated/image ones.
	if b0["model"] != "gpt-6.1-sol" || f.auth[0] != "Bearer key-openai" {
		t.Fatalf("model %v auth %q", b0["model"], f.auth[0])
	}
	if tools := b0["tools"].([]any); tools[0].(map[string]any)["function"].(map[string]any)["name"] != "type_in_terminal" {
		t.Fatalf("tools %v", tools)
	}
	msgs := b1["messages"].([]any)
	last := msgs[len(msgs)-1].(map[string]any)
	if last["role"] != "tool" || last["tool_call_id"] != "call_1" || last["content"] != "typed" {
		t.Fatalf("tool result %v", last)
	}
	if first := msgs[0].(map[string]any); first["role"] != "system" {
		t.Fatalf("first message %v", first)
	}

	// History continues; another provider's history is separate.
	c.Ask(context.Background(), Request{ID: "2", Chat: "tab1", Mode: ModeChat, Prompt: "thanks"}, func(string) {}, nil)
	// system, user, assistant(tool call), tool, assistant, + the new user question
	if n := len(f.bodies[2]["messages"].([]any)); n != 6 {
		t.Fatalf("history has %d messages, want 6", n)
	}
	c.Reset("tab1")
	c.Ask(context.Background(), Request{ID: "3", Chat: "tab1", Mode: ModeChat, Prompt: "fresh"}, func(string) {}, nil)
	if n := len(f.bodies[3]["messages"].([]any)); n != 2 {
		t.Fatalf("after reset %d messages", n)
	}
}

func TestDeepSeekReasoningGoesBack(t *testing.T) {
	f := &fakeCompat{toolInput: `{"command":"ls","run":true}`, reasoning: "user wants a listing"}
	c := compatSetup(t, f, ProviderDeepSeek, "")
	if _, err := c.Ask(context.Background(), Request{ID: "1", Chat: "x", Mode: ModeChat, Prompt: "run ls"}, func(string) {},
		func(ToolCall) (string, bool) { return "exit 0", false }); err != nil {
		t.Fatal(err)
	}
	if f.bodies[0]["model"] != "deepseek-v4-flash" || f.auth[0] != "Bearer key-deepseek" {
		t.Fatalf("model %v auth %q", f.bodies[0]["model"], f.auth[0])
	}
	msgs := f.bodies[1]["messages"].([]any)
	asst := msgs[len(msgs)-2].(map[string]any)
	if asst["role"] != "assistant" || asst["reasoning_content"] != "user wants a listing" {
		t.Fatalf("assistant turn sent back: %v", asst)
	}
}

func TestCompatErrorsAndCommandMode(t *testing.T) {
	f := &fakeCompat{}
	c := compatSetup(t, f, ProviderOpenAI, "gpt-6.1-sol")
	c.Ask(context.Background(), Request{ID: "1", Chat: "cmd", Mode: ModeCommand, Prompt: "list files"}, func(string) {}, func(ToolCall) (string, bool) { return "", false })
	if _, has := f.bodies[0]["tools"]; has || len(f.bodies[0]["messages"].([]any)) != 2 {
		t.Fatalf("command mode body %v", f.bodies[0])
	}

	c.Configure(func() string { return ProviderOpenAI }, func(string) string { return "" }, func(string) string { return "" })
	if _, err := c.Ask(context.Background(), Request{Prompt: "x"}, func(string) {}, nil); err == nil || !strings.Contains(err.Error(), "ChatGPT API key") {
		t.Fatalf("no key: %v", err)
	}

	bad := &fakeCompat{status: 401}
	c = compatSetup(t, bad, ProviderDeepSeek, "deepseek-v4-pro")
	if _, err := c.Ask(context.Background(), Request{ID: "1", Prompt: "x"}, func(string) {}, nil); err == nil || !strings.Contains(err.Error(), "DeepSeek didn't accept your API key") {
		t.Fatalf("401: %v", err)
	}
}

func TestListModelsRanking(t *testing.T) {
	f := &fakeCompat{}
	compatSetup(t, f, ProviderOpenAI, "")
	got, err := ListModels(context.Background(), ProviderOpenAI, "k")
	if err != nil {
		t.Fatal(err)
	}
	if strings.Join(got, ",") != "gpt-6.1-sol,gpt-6.1-sol-2026-09-01,gpt-6-luna,gpt-4o-mini" {
		t.Fatalf("models %v", got)
	}
}
