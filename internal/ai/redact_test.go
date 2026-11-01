package ai

import (
	"strings"
	"testing"
)

func TestRedact(t *testing.T) {
	cases := []struct{ in, want string }{
		{"export ANTHROPIC_API_KEY=sk-ant-api03-abcdefghijklmnopqrstuvwxyz", "export ANTHROPIC_API_KEY=[hidden secret]"},
		{"key sk-or-v1-0123456789abcdef0123456789abcdef here", "key [hidden secret] here"},
		{"OPENAI sk-proj-AbCdEfGhIjKlMnOpQrStUv", "OPENAI [hidden secret]"},
		{"aws AKIAIOSFODNN7EXAMPLE ok", "aws [hidden secret] ok"},
		{"token ghp_0123456789abcdefghijklmnopqrstuvwxyz", "token [hidden secret]"},
		{"Authorization: Bearer abc.def.ghi", "Authorization: Bearer [hidden secret]"},
		{"git clone https://jamal:hunter2secret@github.com/x/y", "git clone https://jamal:[hidden secret]@github.com/x/y"},
		{`DB_PASSWORD="correcthorse"`, `DB_PASSWORD="[hidden secret]"`},
		{"jwt eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.SflKxwRJSMeKKF2QT4fwpM", "jwt [hidden secret]"},
	}
	for _, c := range cases {
		got, n := Redact(c.in)
		if got != c.want || n == 0 {
			t.Errorf("Redact(%q)\n got %q (%d)\nwant %q", c.in, got, n, c.want)
		}
	}
	key := "-----BEGIN OPENSSH PRIVATE KEY-----\nb3BlbnNzaC1rZXktdjEAAAAA\n-----END OPENSSH PRIVATE KEY-----"
	if got, _ := Redact("cat id\n" + key + "\ndone"); got != "cat id\n[hidden secret]\ndone" {
		t.Errorf("private key: %q", got)
	}
	// Ordinary output is left alone.
	plain := "ls -la\ntotal 8\n-rw-r--r-- 1 j staff 12 notes.txt\ngit status: on branch main\npassword reset email sent"
	if got, n := Redact(plain); got != plain || n != 0 {
		t.Errorf("changed ordinary text: %q (%d)", got, n)
	}
}

func TestContextIsRedactedButPromptIsNot(t *testing.T) {
	f := &fakeAPI{reply: []string{"ok"}}
	c := setup(t, f, "claude-opus-5-5")
	var hidden int
	c.OnRedact(func(id string, n int) { hidden += n })
	c.Ask(t.Context(), Request{ID: "1", Chat: "r", Mode: ModeChat,
		Prompt:  "is my key sk-ant-api03-mine0123456789abcdef ok?",
		Context: "$ env\nOPENAI_API_KEY=sk-proj-AbCdEfGhIjKlMnOpQrStUv\nHOME=/Users/j"}, func(string) {}, nil)
	msgs := f.bodies[0]["messages"].([]any)
	text := msgs[0].(map[string]any)["content"].([]any)[0].(map[string]any)["text"].(string)
	if strings.Contains(text, "AbCdEfGh") || !strings.Contains(text, "OPENAI_API_KEY=[hidden secret]") {
		t.Fatalf("context not redacted: %q", text)
	}
	if !strings.Contains(text, "sk-ant-api03-mine0123456789abcdef") {
		t.Fatalf("the user's own question was changed: %q", text)
	}
	if hidden != 1 {
		t.Fatalf("hidden = %d", hidden)
	}
}
