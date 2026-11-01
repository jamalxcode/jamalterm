package ai

import (
	"context"
	"crypto/rand"
	"crypto/sha256"
	"encoding/base64"
	"encoding/json"
	"errors"
	"fmt"
	"net"
	"net/http"
	"net/url"
	"strings"
	"time"
)

// OpenRouter sign-in: OpenRouter's public OAuth PKCE flow, which needs no
// app registration. The user approves in the browser and OpenRouter
// returns an API key tied to their account.
//
//	1. open https://openrouter.ai/auth?callback_url=http://localhost:PORT/callback
//	   &code_challenge=…&code_challenge_method=S256
//	2. OpenRouter redirects to the callback with ?code=…
//	3. POST https://openrouter.ai/api/v1/auth/keys {code, code_verifier,
//	   code_challenge_method} → {"key": "…"}

var (
	openRouterAuthURL = "https://openrouter.ai/auth"
	openRouterKeysURL = "https://openrouter.ai/api/v1/auth/keys"
)

// OpenRouterSignIn runs the browser sign-in and returns the new API key.
// openURL opens a URL in the user's browser.
func OpenRouterSignIn(ctx context.Context, openURL func(string)) (string, error) {
	verifier, challenge, err := pkcePair()
	if err != nil {
		return "", err
	}
	ln, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		return "", fmt.Errorf("couldn't start the sign-in listener: %w", err)
	}
	defer ln.Close()
	callback := fmt.Sprintf("http://localhost:%d/callback", ln.Addr().(*net.TCPAddr).Port)

	codes := make(chan string, 1)
	srv := &http.Server{
		ReadHeaderTimeout: 10 * time.Second,
		Handler: http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			if r.URL.Path != "/callback" {
				http.NotFound(w, r)
				return
			}
			code := r.URL.Query().Get("code")
			w.Header().Set("Content-Type", "text/html; charset=utf-8")
			if code == "" {
				w.WriteHeader(http.StatusBadRequest)
				fmt.Fprint(w, page("Sign-in didn't finish", "No code came back from OpenRouter. Close this tab and try again from jamalterm."))
				return
			}
			fmt.Fprint(w, page("Signed in to OpenRouter", "You can close this tab and go back to jamalterm."))
			select {
			case codes <- code:
			default:
			}
		}),
	}
	go srv.Serve(ln)
	defer srv.Close()

	q := url.Values{}
	q.Set("callback_url", callback)
	q.Set("code_challenge", challenge)
	q.Set("code_challenge_method", "S256")
	openURL(openRouterAuthURL + "?" + q.Encode())

	var code string
	select {
	case code = <-codes:
	case <-time.After(5 * time.Minute):
		return "", errors.New("sign-in timed out; try again")
	case <-ctx.Done():
		return "", ctx.Err()
	}
	return exchangeOpenRouterCode(ctx, code, verifier)
}

func exchangeOpenRouterCode(ctx context.Context, code, verifier string) (string, error) {
	body, _ := json.Marshal(map[string]string{"code": code, "code_verifier": verifier, "code_challenge_method": "S256"})
	req, _ := http.NewRequestWithContext(ctx, "POST", openRouterKeysURL, strings.NewReader(string(body)))
	req.Header.Set("Content-Type", "application/json")
	resp, err := (&http.Client{Timeout: 30 * time.Second}).Do(req)
	if err != nil {
		return "", fmt.Errorf("couldn't reach OpenRouter: %w", err)
	}
	defer resp.Body.Close()
	switch resp.StatusCode {
	case 200, 201:
	case 403:
		return "", errors.New("OpenRouter rejected the sign-in code (it may have expired); try again")
	default:
		// Don't show the body: it may echo the code.
		return "", fmt.Errorf("OpenRouter sign-in failed (HTTP %d)", resp.StatusCode)
	}
	var out struct {
		Key string `json:"key"`
	}
	if err := json.NewDecoder(resp.Body).Decode(&out); err != nil || out.Key == "" {
		return "", errors.New("OpenRouter's answer had no key")
	}
	return out.Key, nil
}

// pkcePair makes a PKCE verifier and its S256 challenge.
func pkcePair() (verifier, challenge string, err error) {
	b := make([]byte, 32)
	if _, err := rand.Read(b); err != nil {
		return "", "", err
	}
	verifier = base64.RawURLEncoding.EncodeToString(b)
	sum := sha256.Sum256([]byte(verifier))
	return verifier, base64.RawURLEncoding.EncodeToString(sum[:]), nil
}

func page(title, msg string) string {
	return `<!doctype html><meta charset="utf-8"><title>jamalterm</title>
<body style="font:16px -apple-system,sans-serif;background:#15161e;color:#c8d3f5;display:grid;place-items:center;height:100vh;margin:0">
<div style="text-align:center"><div style="font-size:40px;color:#ff8fb1">&gt;_</div><h2>` + title + `</h2><p>` + msg + `</p></div>`
}
