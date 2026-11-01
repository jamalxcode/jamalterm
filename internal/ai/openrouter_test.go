package ai

import (
	"context"
	"crypto/sha256"
	"encoding/base64"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"net/url"
	"strings"
	"testing"
)

func TestOpenRouterSignIn(t *testing.T) {
	var challenge string
	keys := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		var in map[string]string
		json.NewDecoder(r.Body).Decode(&in)
		sum := sha256.Sum256([]byte(in["code_verifier"]))
		if r.Method != "POST" || in["code"] != "the-code" || in["code_challenge_method"] != "S256" ||
			base64.RawURLEncoding.EncodeToString(sum[:]) != challenge {
			w.WriteHeader(403)
			return
		}
		json.NewEncoder(w).Encode(map[string]string{"key": "sk-or-v1-new", "user_id": "u"})
	}))
	defer keys.Close()
	openRouterAuthURL, openRouterKeysURL = "https://openrouter.test/auth", keys.URL

	// The "browser": check the URL, then come back to the callback with a code.
	browser := func(u string) {
		p, err := url.Parse(u)
		if err != nil || !strings.HasPrefix(u, "https://openrouter.test/auth?") {
			t.Errorf("auth URL %q", u)
			return
		}
		q := p.Query()
		challenge = q.Get("code_challenge")
		cb := q.Get("callback_url")
		if q.Get("code_challenge_method") != "S256" || len(challenge) != 43 || !strings.HasPrefix(cb, "http://localhost:") || !strings.HasSuffix(cb, "/callback") {
			t.Errorf("auth params %v", q)
			return
		}
		go func() {
			resp, err := http.Get(cb + "?code=the-code")
			if err != nil {
				t.Errorf("callback: %v", err)
				return
			}
			resp.Body.Close()
			if resp.StatusCode != 200 {
				t.Errorf("callback status %d", resp.StatusCode)
			}
		}()
	}
	key, err := OpenRouterSignIn(context.Background(), browser)
	if err != nil || key != "sk-or-v1-new" {
		t.Fatalf("key %q err %v", key, err)
	}

	// A wrong verifier is rejected by the server and reported clearly.
	if _, err := exchangeOpenRouterCode(context.Background(), "the-code", "not-the-verifier"); err == nil || !strings.Contains(err.Error(), "rejected") {
		t.Fatalf("bad verifier: %v", err)
	}
}

func TestOpenRouterChat(t *testing.T) {
	var headers http.Header
	f := &fakeCompat{}
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		headers = r.Header.Clone()
		f.ServeHTTP(w, r)
	}))
	defer srv.Close()
	t.Setenv("JAMALTERM_OPENROUTER_BASE_URL", srv.URL+"/api/v1")
	c := New(func() string { return "" }, func() string { return "" })
	c.Configure(func() string { return ProviderOpenRouter }, func(p string) string { return "sk-or-" + p }, func(string) string { return "" })
	got, err := c.Ask(context.Background(), Request{ID: "1", Chat: "o", Mode: ModeChat, Prompt: "hi"}, func(string) {}, nil)
	if err != nil || got != "Hello there." {
		t.Fatalf("%q %v", got, err)
	}
	if f.bodies[0]["model"] != "openrouter/auto" || headers.Get("X-Title") != "jamalterm" || headers.Get("Authorization") != "Bearer sk-or-openrouter" {
		t.Fatalf("model %v headers %v", f.bodies[0]["model"], headers)
	}
}
