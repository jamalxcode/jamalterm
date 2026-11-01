package ai

import (
	"regexp"
	"strings"
)

// Redact hides things that look like secrets in terminal text before it is
// sent to an AI provider, and reports how many it hid. It is a safety net,
// not a guarantee: it catches common key and token formats.
func Redact(text string) (string, int) {
	n := 0
	for _, r := range redactions {
		text = r.re.ReplaceAllStringFunc(text, func(m string) string {
			if strings.Contains(m, "[hidden") {
				return m // already hidden by an earlier rule
			}
			n++
			if r.keep == nil {
				return "[hidden secret]"
			}
			// Keep the name (e.g. "password=") and hide only the value.
			sub := r.re.FindStringSubmatch(m)
			tail := ""
			if len(sub) > 2 {
				tail = sub[2]
			}
			return sub[1] + "[hidden secret]" + tail
		})
	}
	return text, n
}

type redaction struct {
	re   *regexp.Regexp
	keep *struct{} // non-nil: group 1 is a prefix to keep
}

var keep = &struct{}{}

var redactions = []redaction{
	// Private keys (whole block).
	{re: regexp.MustCompile(`-----BEGIN [A-Z ]*PRIVATE KEY-----[\s\S]*?-----END [A-Z ]*PRIVATE KEY-----`)},
	// Provider API keys and tokens.
	{re: regexp.MustCompile(`\bsk-(?:ant|or|proj|svcacct|admin)-[A-Za-z0-9_\-]{16,}`)},
	{re: regexp.MustCompile(`\bsk-[A-Za-z0-9]{20,}\b`)},
	{re: regexp.MustCompile(`\bxai-[A-Za-z0-9]{20,}\b`)},
	{re: regexp.MustCompile(`\bAIza[0-9A-Za-z_\-]{35}\b`)},
	{re: regexp.MustCompile(`\b(?:sk|rk)_(?:live|test)_[0-9A-Za-z]{16,}\b`)},
	{re: regexp.MustCompile(`\b(?:gh[pousr]_[A-Za-z0-9]{30,}|github_pat_[A-Za-z0-9_]{30,})\b`)},
	{re: regexp.MustCompile(`\bglpat-[A-Za-z0-9_\-]{20,}\b`)},
	{re: regexp.MustCompile(`\bxox[abprs]-[A-Za-z0-9\-]{10,}\b`)},
	{re: regexp.MustCompile(`\b(?:AKIA|ASIA)[0-9A-Z]{16}\b`)},
	{re: regexp.MustCompile(`\beyJ[A-Za-z0-9_\-]{10,}\.[A-Za-z0-9_\-]{10,}\.[A-Za-z0-9_\-]{10,}\b`)},
	// "Authorization: Bearer <token>"
	{re: regexp.MustCompile(`(?i)(authorization:\s*(?:bearer|token|basic)\s+)[^\s"']+`), keep: keep},
	// user:password@ in URLs
	{re: regexp.MustCompile(`(://[^\s:/@]+:)[^\s@/]+(@)`), keep: keep},
	// NAME=value / NAME: value for password-like names
	{re: regexp.MustCompile(`(?i)(\b[A-Z0-9_]*(?:password|passwd|secret|token|api[_-]?key|access[_-]?key|private[_-]?key)[A-Z0-9_]*\b["']?\s*[=:]\s*["']?)[^\s"']{6,}`), keep: keep},
}
