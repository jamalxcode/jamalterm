package ai

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"regexp"
	"sort"
	"strconv"
	"strings"

	"github.com/openai/openai-go/v3"
	oaioption "github.com/openai/openai-go/v3/option"
	"github.com/openai/openai-go/v3/shared"
)

// Providers the assistant can use.
const (
	ProviderAnthropic = "anthropic" // Claude (default)
	ProviderOpenAI    = "openai"    // ChatGPT models via the OpenAI API
	ProviderDeepSeek  = "deepseek"  // DeepSeek (OpenAI-compatible API)
)

// ProviderInfo describes a provider for the UI.
type ProviderInfo struct {
	ID      string `json:"id"`
	Name    string `json:"name"`    // "ChatGPT"
	KeyURL  string `json:"keyUrl"`  // where to create an API key
	KeyHint string `json:"keyHint"` // what keys look like
}

var Providers = []ProviderInfo{
	{ProviderAnthropic, "Claude", "https://console.anthropic.com/settings/keys", "sk-ant-…"},
	{ProviderOpenAI, "ChatGPT", "https://platform.openai.com/api-keys", "sk-…"},
	{ProviderDeepSeek, "DeepSeek", "https://platform.deepseek.com/api_keys", "sk-…"},
}

func providerName(id string) string {
	for _, p := range Providers {
		if p.ID == id {
			return p.Name
		}
	}
	return id
}

// compatBase is the API address for an OpenAI-compatible provider.
func compatBase(provider string) string {
	switch provider {
	case ProviderDeepSeek:
		if b := os.Getenv("JAMALTERM_DEEPSEEK_BASE_URL"); b != "" {
			return b
		}
		return "https://api.deepseek.com"
	default:
		if b := os.Getenv("JAMALTERM_OPENAI_BASE_URL"); b != "" {
			return b
		}
		return "https://api.openai.com/v1"
	}
}

func compatClient(provider, key string) openai.Client {
	return openai.NewClient(oaioption.WithAPIKey(key), oaioption.WithBaseURL(compatBase(provider)))
}

// ListModels returns the chat models a provider offers to this key, newest
// first.
func ListModels(ctx context.Context, provider, key string) ([]string, error) {
	if key == "" {
		return nil, errNoProviderKey(provider)
	}
	client := compatClient(provider, key)
	page, err := client.Models.List(ctx)
	if err != nil {
		return nil, compatError(provider, err)
	}
	var out []string
	for {
		for _, m := range page.Data {
			if isChatModel(provider, m.ID) {
				out = append(out, m.ID)
			}
		}
		next, err := page.GetNextPage()
		if err != nil || next == nil {
			break
		}
		page = next
	}
	sort.SliceStable(out, func(i, j int) bool { return rankModel(out[i]) > rankModel(out[j]) })
	return out, nil
}

// Skip models that can't chat (images, audio, embeddings, …).
var notChat = regexp.MustCompile(`(?i)(embed|image|dall|tts|whisper|audio|realtime|transcri|speech|moderation|search|live|sora|vision-preview|instruct|davinci|babbage)`)

func isChatModel(provider, id string) bool {
	if notChat.MatchString(id) {
		return false
	}
	if provider == ProviderOpenAI {
		return strings.HasPrefix(id, "gpt-") || strings.HasPrefix(id, "chatgpt-") || regexp.MustCompile(`^o\d`).MatchString(id)
	}
	return true
}

var versionRe = regexp.MustCompile(`(\d+(?:\.\d+)?)`)

// rankModel sorts newer, general-purpose models first.
func rankModel(id string) float64 {
	v := 0.0
	if m := versionRe.FindString(id); m != "" {
		v, _ = strconv.ParseFloat(m, 64)
	}
	score := v * 100
	l := strings.ToLower(id)
	for _, small := range []string{"mini", "nano", "luna", "lite", "flash"} {
		if strings.Contains(l, small) {
			score -= 30
		}
	}
	if strings.Contains(l, "pro") {
		score -= 10 // slow and pricey for terminal help
	}
	if regexp.MustCompile(`\d{4}-\d{2}-\d{2}|\d{4}$`).MatchString(l) {
		score -= 5 // dated snapshots after the moving alias
	}
	return score
}

// resolveModel picks the configured model, or for "auto" the newest
// general chat model this key can use.
func (c *Client) resolveModel(ctx context.Context, provider, key string) (string, error) {
	if m := strings.TrimSpace(c.modelFor(provider)); m != "" {
		return m, nil
	}
	c.mu.Lock()
	cached := c.autoModel[provider]
	c.mu.Unlock()
	if cached != "" {
		return cached, nil
	}
	if provider == ProviderDeepSeek {
		// DeepSeek's current general model (per its docs).
		return "deepseek-v4-flash", nil
	}
	models, err := ListModels(ctx, provider, key)
	if err != nil {
		return "", err
	}
	if len(models) == 0 {
		return "", fmt.Errorf("%s offered no chat models to this API key; pick one in Settings", providerName(provider))
	}
	c.mu.Lock()
	c.autoModel[provider] = models[0]
	c.mu.Unlock()
	return models[0], nil
}

func errNoProviderKey(provider string) error {
	return fmt.Errorf("add your %s API key in Settings (⌘,) to use AI features", providerName(provider))
}

var compatTool = openai.ChatCompletionFunctionTool(shared.FunctionDefinitionParam{
	Name:        typeToolName,
	Description: openai.String(typeTool.OfTool.Description.Value),
	Parameters: shared.FunctionParameters{
		"type": "object",
		"properties": map[string]any{
			"command": map[string]any{"type": "string", "description": "The exact command line for the user's shell."},
			"run":     map[string]any{"type": "boolean", "description": "true to run it (after the user confirms), false to only type it."},
		},
		"required":             []string{"command", "run"},
		"additionalProperties": false,
	},
})

// askCompat answers with an OpenAI-compatible provider (ChatGPT, DeepSeek),
// with the same prompts and type_in_terminal tool as Claude.
func (c *Client) askCompat(ctx context.Context, provider string, req Request, userText string, onDelta func(string), onTool ToolHandler) (string, error) {
	key := c.keyFor(provider)
	if key == "" {
		return "", errNoProviderKey(provider)
	}
	model, err := c.resolveModel(ctx, provider, key)
	if err != nil {
		return "", err
	}
	client := compatClient(provider, key)

	chatKey := provider + ":" + req.Chat
	var messages []openai.ChatCompletionMessageParamUnion
	params := openai.ChatCompletionNewParams{Model: shared.ChatModel(model)}
	if req.Mode == ModeCommand {
		messages = []openai.ChatCompletionMessageParamUnion{openai.SystemMessage(commandSystem), openai.UserMessage(userText)}
	} else {
		c.mu.Lock()
		history := append([]openai.ChatCompletionMessageParamUnion(nil), c.compatChats[chatKey]...)
		c.mu.Unlock()
		if len(history) == 0 {
			history = []openai.ChatCompletionMessageParamUnion{openai.SystemMessage(chatSystem)}
		}
		messages = append(history, openai.UserMessage(userText))
		if onTool != nil {
			params.Tools = []openai.ChatCompletionToolUnionParam{compatTool}
		}
	}

	var answer strings.Builder
	for round := 0; ; round++ {
		if round > 0 && answer.Len() > 0 {
			onDelta("\n\n")
			answer.WriteString("\n\n")
		}
		params.Messages = messages
		msg, reasoning, finish, err := compatStream(ctx, client, params, onDelta)
		if err != nil {
			return answer.String(), compatError(provider, err)
		}
		if msg.Refusal != "" {
			return "", fmt.Errorf("%s declined to answer this one", providerName(provider))
		}
		answer.WriteString(msg.Content)
		asst := msg.ToAssistantMessageParam()
		if reasoning != "" {
			// DeepSeek's thinking mode wants its reasoning back in tool loops.
			asst.SetExtraFields(map[string]any{"reasoning_content": reasoning})
		}
		messages = append(messages, openai.ChatCompletionMessageParamUnion{OfAssistant: &asst})

		if len(msg.ToolCalls) == 0 || round >= 5 {
			break
		}
		for _, tc := range msg.ToolCalls {
			fn := tc.Function
			if fn.Name != typeToolName || onTool == nil {
				messages = append(messages, openai.ToolMessage("unknown tool", tc.ID))
				continue
			}
			command, run, ok := parseToolInput(fn.Arguments)
			if !ok || finish == "length" {
				messages = append(messages, openai.ToolMessage(invalidInput(fn.Arguments), tc.ID))
				continue
			}
			out, _ := onTool(ToolCall{ID: tc.ID, Command: command, Run: run})
			messages = append(messages, openai.ToolMessage(out, tc.ID))
		}
		if ctx.Err() != nil {
			return "", context.Canceled
		}
	}

	if req.Mode != ModeCommand {
		c.mu.Lock()
		c.compatChats[chatKey] = messages
		c.mu.Unlock()
	}
	return answer.String(), nil
}

// compatStream runs one streamed chat completion.
func compatStream(ctx context.Context, client openai.Client, params openai.ChatCompletionNewParams, onDelta func(string)) (openai.ChatCompletionMessage, string, string, error) {
	stream := client.Chat.Completions.NewStreaming(ctx, params)
	acc := openai.ChatCompletionAccumulator{}
	var reasoning strings.Builder
	for stream.Next() {
		chunk := stream.Current()
		acc.AddChunk(chunk)
		if len(chunk.Choices) == 0 {
			continue
		}
		d := chunk.Choices[0].Delta
		if d.Content != "" {
			onDelta(d.Content)
		}
		// Not in the SDK's types: DeepSeek sends it as an extra field.
		if f, ok := d.JSON.ExtraFields["reasoning_content"]; ok && f.Raw() != "" && f.Raw() != "null" {
			var s string
			if json.Unmarshal([]byte(f.Raw()), &s) == nil {
				reasoning.WriteString(s)
			}
		}
	}
	if err := stream.Err(); err != nil {
		if ctx.Err() != nil {
			return openai.ChatCompletionMessage{}, "", "", context.Canceled
		}
		return openai.ChatCompletionMessage{}, "", "", err
	}
	if len(acc.Choices) == 0 {
		return openai.ChatCompletionMessage{}, "", "", errors.New("empty answer")
	}
	ch := acc.Choices[0]
	return ch.Message, reasoning.String(), string(ch.FinishReason), nil
}

func compatError(provider string, err error) error {
	if errors.Is(err, context.Canceled) {
		return err
	}
	name := providerName(provider)
	var apiErr *openai.Error
	if errors.As(err, &apiErr) {
		switch apiErr.StatusCode {
		case 401, 403:
			return fmt.Errorf("%s didn't accept your API key; check it in Settings (⌘,)", name)
		case 402:
			return fmt.Errorf("your %s account is out of credit (insufficient balance)", name)
		case 404:
			return fmt.Errorf("that model isn't available to your %s key; pick another in Settings (⌘,)", name)
		case 429:
			return fmt.Errorf("%s rate limit or quota reached; try again in a moment, or check your plan's billing", name)
		case 500, 502, 503, 529:
			return fmt.Errorf("%s is busy right now; try again in a moment", name)
		}
		return fmt.Errorf("%s API error %d", name, apiErr.StatusCode)
	}
	return fmt.Errorf("couldn't reach %s: %v", name, err)
}
