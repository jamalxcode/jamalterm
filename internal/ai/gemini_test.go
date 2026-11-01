package ai

import (
	"context"
	"fmt"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"golang.org/x/oauth2"
)

// fakeGemini serves the Gemini API (models/… ids) and Vertex AI's model list.
type fakeGemini struct {
	fakeCompat
	paths []string
}

func (f *fakeGemini) ServeHTTP(w http.ResponseWriter, r *http.Request) {
	f.mu.Lock()
	f.paths = append(f.paths, r.URL.Path)
	f.mu.Unlock()
	w.Header().Set("Content-Type", "application/json")
	switch {
	case r.Method == "GET" && strings.HasSuffix(r.URL.Path, "/openai/models"):
		fmt.Fprint(w, `{"object":"list","data":[{"id":"models/gemini-3.5-pro","object":"model"},{"id":"models/gemini-3.8-flash","object":"model"},
		  {"id":"models/gemini-3.8-flash-lite","object":"model"},{"id":"models/text-embedding-005","object":"model"},{"id":"models/gemini-3.8-flash-image","object":"model"}]}`)
	case r.Method == "GET" && strings.HasSuffix(r.URL.Path, "/publishers/google/models"):
		if r.Header.Get("Authorization") != "Bearer goog-token" || r.Header.Get("x-goog-user-project") != "my-proj" {
			w.WriteHeader(403)
			return
		}
		fmt.Fprint(w, `{"publisherModels":[{"name":"publishers/google/models/gemini-3.8-flash"},{"name":"publishers/google/models/gemini-3.5-pro"},{"name":"publishers/google/models/imagen-5"}]}`)
	default:
		f.fakeCompat.ServeHTTP(w, r)
	}
}

func geminiSetup(t *testing.T, key, model, project string) (*fakeGemini, *Client) {
	f := &fakeGemini{}
	srv := httptest.NewServer(f)
	t.Cleanup(srv.Close)
	t.Setenv("JAMALTERM_GEMINI_BASE_URL", srv.URL+"/v1beta/openai/")
	t.Setenv("JAMALTERM_VERTEX_BASE_URL", srv.URL+"/v1/projects/{project}/locations/global/endpoints/openapi")
	t.Setenv("JAMALTERM_VERTEX_MODELS_URL", srv.URL+"/v1beta1/publishers/google/models")
	c := New(func() string { return "" }, func() string { return "" })
	c.Configure(func() string { return ProviderGemini }, func(string) string { return key }, func(string) string { return model })
	c.ConfigureGoogle(func() string { return project })
	return f, c
}

// signInWithGoogle fakes what `gcloud auth application-default login` leaves.
func signInWithGoogle(t *testing.T, quotaProject string) {
	dir := t.TempDir()
	adc := filepath.Join(dir, "adc.json")
	os.WriteFile(adc, []byte(`{"type":"authorized_user","client_id":"x","client_secret":"y","refresh_token":"z","quota_project_id":"`+quotaProject+`"}`), 0o600)
	t.Setenv("GOOGLE_APPLICATION_CREDENTIALS", adc)
	t.Setenv("CLOUDSDK_CONFIG", dir)
	old := googleTokenSource
	googleTokenSource = func(context.Context) (oauth2.TokenSource, error) {
		return oauth2.StaticTokenSource(&oauth2.Token{AccessToken: "goog-token"}), nil
	}
	t.Cleanup(func() { googleTokenSource = old })
}

func TestGeminiAPIKey(t *testing.T) {
	t.Setenv("GOOGLE_APPLICATION_CREDENTIALS", filepath.Join(t.TempDir(), "none.json"))
	f, c := geminiSetup(t, "AIza-key", "", "")
	got, err := c.Ask(context.Background(), Request{ID: "1", Chat: "g", Mode: ModeChat, Prompt: "hi"}, func(string) {}, nil)
	if err != nil || got != "Hello there." {
		t.Fatalf("%q %v", got, err)
	}
	if f.bodies[0]["model"] != "gemini-3.8-flash" || f.auth[0] != "Bearer AIza-key" {
		t.Fatalf("model %v auth %q", f.bodies[0]["model"], f.auth[0])
	}
	if !strings.HasSuffix(f.paths[len(f.paths)-1], "/v1beta/openai/chat/completions") {
		t.Fatalf("path %v", f.paths)
	}
	models, _ := c.ListModelsFor(context.Background(), ProviderGemini)
	if strings.Join(models, ",") != "gemini-3.8-flash,gemini-3.5-pro,gemini-3.8-flash-lite" {
		t.Fatalf("models %v", models)
	}
}

func TestGeminiGoogleSignIn(t *testing.T) {
	signInWithGoogle(t, "my-proj")
	if a := GoogleSignIn(""); !a.SignedIn || a.Project != "my-proj" {
		t.Fatalf("account %+v", a)
	}
	f, c := geminiSetup(t, "", "", "")
	got, err := c.Ask(context.Background(), Request{ID: "1", Chat: "g", Mode: ModeChat, Prompt: "hi"}, func(string) {}, nil)
	if err != nil || got != "Hello there." {
		t.Fatalf("%q %v", got, err)
	}
	if f.bodies[0]["model"] != "google/gemini-3.8-flash" || f.auth[0] != "Bearer goog-token" {
		t.Fatalf("model %v auth %q", f.bodies[0]["model"], f.auth[0])
	}
	if p := f.paths[len(f.paths)-1]; p != "/v1/projects/my-proj/locations/global/endpoints/openapi/chat/completions" {
		t.Fatalf("path %q", p)
	}

	// A model typed in Settings gets the Vertex form; the Settings project wins.
	f2, c2 := geminiSetup(t, "", "gemini-3.5-pro", "other-proj")
	c2.Ask(context.Background(), Request{ID: "1", Chat: "g", Mode: ModeChat, Prompt: "hi"}, func(string) {}, nil)
	if f2.bodies[0]["model"] != "google/gemini-3.5-pro" || !strings.Contains(f2.paths[len(f2.paths)-1], "/projects/other-proj/") {
		t.Fatalf("model %v path %v", f2.bodies[0]["model"], f2.paths)
	}

	// A saved API key wins over the sign-in.
	f3, c3 := geminiSetup(t, "AIza-key", "google/gemini-3.5-pro", "")
	c3.Ask(context.Background(), Request{ID: "1", Chat: "g", Mode: ModeChat, Prompt: "hi"}, func(string) {}, nil)
	if f3.auth[0] != "Bearer AIza-key" || f3.bodies[0]["model"] != "gemini-3.5-pro" {
		t.Fatalf("key should win: %q %v", f3.auth[0], f3.bodies[0]["model"])
	}
}

func TestGeminiNeedsProjectOrCredentials(t *testing.T) {
	signInWithGoogle(t, "")
	_, c := geminiSetup(t, "", "", "")
	if _, err := c.Ask(context.Background(), Request{Prompt: "x"}, func(string) {}, nil); err == nil || !strings.Contains(err.Error(), "no Google Cloud project") {
		t.Fatalf("no project: %v", err)
	}
	// gcloud's default project is used when nothing else is set.
	dir := os.Getenv("CLOUDSDK_CONFIG")
	os.MkdirAll(filepath.Join(dir, "configurations"), 0o755)
	os.WriteFile(filepath.Join(dir, "configurations", "config_default"), []byte("[core]\naccount = me@example.com\nproject = from-gcloud\n"), 0o644)
	if a := GoogleSignIn(""); a.Project != "from-gcloud" {
		t.Fatalf("project %q", a.Project)
	}

	t.Setenv("GOOGLE_APPLICATION_CREDENTIALS", filepath.Join(t.TempDir(), "none.json"))
	if _, err := c.Ask(context.Background(), Request{Prompt: "x"}, func(string) {}, nil); err == nil || !strings.Contains(err.Error(), "sign in with Google or add a Gemini API key") {
		t.Fatalf("nothing set up: %v", err)
	}
}
