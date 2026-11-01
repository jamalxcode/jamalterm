package ai

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"net/http"
	"net/url"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"time"

	"golang.org/x/oauth2"
	"golang.org/x/oauth2/google"
)

// Gemini works two ways:
//
//   - API key (from aistudio.google.com) → the Gemini API's
//     OpenAI-compatible endpoint.
//   - Google sign-in → Gemini on Vertex AI in the user's Google Cloud
//     project. The sign-in is Google's own: `gcloud auth
//     application-default login` (Application Default Credentials), and
//     Google's oauth2 library refreshes the token.
//
// A saved API key wins over the sign-in, as with Claude.

const ProviderGemini = "gemini"

var errNoGoogle = errors.New("not signed in with Google")

const cloudPlatformScope = "https://www.googleapis.com/auth/cloud-platform"

// GoogleAccount describes the Google sign-in, if any.
type GoogleAccount struct {
	SignedIn bool   `json:"signedIn"`
	Project  string `json:"project"` // the project Gemini will use ("" = none found)
}

// adcFile is where `gcloud auth application-default login` saves the sign-in.
func adcFile() string {
	if p := os.Getenv("GOOGLE_APPLICATION_CREDENTIALS"); p != "" {
		return p
	}
	if dir := os.Getenv("CLOUDSDK_CONFIG"); dir != "" {
		return filepath.Join(dir, "application_default_credentials.json")
	}
	home, _ := os.UserHomeDir()
	return filepath.Join(home, ".config", "gcloud", "application_default_credentials.json")
}

func gcloudDir() string {
	if dir := os.Getenv("CLOUDSDK_CONFIG"); dir != "" {
		return dir
	}
	home, _ := os.UserHomeDir()
	return filepath.Join(home, ".config", "gcloud")
}

// GoogleSignIn reports the sign-in and which project would be used. It
// doesn't contact Google.
func GoogleSignIn(projectSetting string) GoogleAccount {
	data, err := os.ReadFile(adcFile())
	if err != nil {
		return GoogleAccount{}
	}
	return GoogleAccount{SignedIn: true, Project: googleProject(projectSetting, data)}
}

// googleProject: the Settings value, else the sign-in's quota project,
// else gcloud's default project.
func googleProject(setting string, adc []byte) string {
	if p := strings.TrimSpace(setting); p != "" {
		return p
	}
	var f struct {
		Quota   string `json:"quota_project_id"`
		Project string `json:"project_id"`
	}
	if json.Unmarshal(adc, &f) == nil {
		if f.Quota != "" {
			return f.Quota
		}
		if f.Project != "" {
			return f.Project
		}
	}
	active := "default"
	if b, err := os.ReadFile(filepath.Join(gcloudDir(), "active_config")); err == nil && strings.TrimSpace(string(b)) != "" {
		active = strings.TrimSpace(string(b))
	}
	if b, err := os.ReadFile(filepath.Join(gcloudDir(), "configurations", "config_"+active)); err == nil {
		section := ""
		for _, line := range strings.Split(string(b), "\n") {
			line = strings.TrimSpace(line)
			if strings.HasPrefix(line, "[") {
				section = line
				continue
			}
			if k, v, ok := strings.Cut(line, "="); ok && section == "[core]" && strings.TrimSpace(k) == "project" {
				return strings.TrimSpace(v)
			}
		}
	}
	return ""
}

// googleTokenSource can be replaced in tests.
var googleTokenSource = func(ctx context.Context) (oauth2.TokenSource, error) {
	creds, err := google.FindDefaultCredentials(ctx, cloudPlatformScope)
	if err != nil {
		return nil, err
	}
	return creds.TokenSource, nil
}

// googleAccess returns a fresh access token and the project to bill.
func googleAccess(ctx context.Context, projectSetting string) (token, project string, err error) {
	acct := GoogleSignIn(projectSetting)
	if !acct.SignedIn {
		return "", "", errNoGoogle
	}
	if acct.Project == "" {
		return "", "", errors.New("signed in with Google, but no Google Cloud project is set; add one in Settings → AI assistant → Gemini")
	}
	ts, err := googleTokenSource(ctx)
	if err != nil {
		return "", "", fmt.Errorf("couldn't use your Google sign-in (%v); sign in again", err)
	}
	tok, err := ts.Token()
	if err != nil {
		return "", "", fmt.Errorf("your Google sign-in has expired or was revoked; sign in again (%v)", err)
	}
	return tok.AccessToken, acct.Project, nil
}

func vertexBase(project string) string {
	if b := os.Getenv("JAMALTERM_VERTEX_BASE_URL"); b != "" {
		return strings.ReplaceAll(b, "{project}", url.PathEscape(project))
	}
	return "https://aiplatform.googleapis.com/v1/projects/" + url.PathEscape(project) + "/locations/global/endpoints/openapi"
}

func vertexModelsURL() string {
	if u := os.Getenv("JAMALTERM_VERTEX_MODELS_URL"); u != "" {
		return u
	}
	return "https://aiplatform.googleapis.com/v1beta1/publishers/google/models"
}

// listVertexModels lists Google's Gemini models on Vertex AI, newest first,
// as "google/gemini-…".
func listVertexModels(ctx context.Context, token, project string) ([]string, error) {
	var out []string
	pageToken := ""
	for page := 0; page < 10; page++ {
		u := vertexModelsURL() + "?pageSize=200"
		if pageToken != "" {
			u += "&pageToken=" + url.QueryEscape(pageToken)
		}
		req, _ := http.NewRequestWithContext(ctx, "GET", u, nil)
		req.Header.Set("Authorization", "Bearer "+token)
		req.Header.Set("x-goog-user-project", project)
		resp, err := (&http.Client{Timeout: 20 * time.Second}).Do(req)
		if err != nil {
			return nil, fmt.Errorf("couldn't reach Google Cloud: %v", err)
		}
		var body struct {
			Models []struct {
				Name string `json:"name"`
			} `json:"publisherModels"`
			Next string `json:"nextPageToken"`
		}
		err = json.NewDecoder(resp.Body).Decode(&body)
		resp.Body.Close()
		if resp.StatusCode == 403 {
			return nil, fmt.Errorf("Google Cloud refused (403): turn on the Vertex AI API for project %s (Settings has a button), or check your access", project)
		}
		if resp.StatusCode != 200 || err != nil {
			return nil, fmt.Errorf("couldn't list Gemini models on Vertex AI (HTTP %d)", resp.StatusCode)
		}
		for _, m := range body.Models {
			id := m.Name[strings.LastIndex(m.Name, "/")+1:]
			if isChatModel(ProviderGemini, id) {
				out = append(out, id)
			}
		}
		if body.Next == "" {
			break
		}
		pageToken = body.Next
	}
	sort.SliceStable(out, func(i, j int) bool { return rankModel(out[i]) > rankModel(out[j]) })
	for i, id := range out {
		out[i] = "google/" + id
	}
	return out, nil
}

// geminiModelFor writes a model name the way each Gemini endpoint expects.
func geminiModelFor(model string, vertex bool) string {
	m := strings.TrimPrefix(strings.TrimPrefix(model, "models/"), "google/")
	if vertex {
		return "google/" + m
	}
	return m
}
