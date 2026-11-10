package ai

import (
	"regexp"
	"strings"
)

// Risk says how careful to be before running a command an AI suggested.
// Terminal output is sent to the AI, so text on screen (a log, a README, a
// web page you cat) could try to talk it into running something harmful.
// The Run button always asks; risky commands ask twice and say why.
type Risk struct {
	Level   string   `json:"level"`   // "", "caution" or "danger"
	Reasons []string `json:"reasons"` // plain-language explanations
}

type riskRule struct {
	re     *regexp.Regexp
	level  string
	reason string
}

var riskRules = []riskRule{
	{regexp.MustCompile(`\brm\s+(-[a-zA-Z]*[rRf][a-zA-Z]*\s+)+(/|~|\$HOME|\*|\.\.?)(\s|/|$)`), "danger", "deletes files recursively from a broad location (/, ~, * or ..)"},
	{regexp.MustCompile(`\brm\s+(-[a-zA-Z]*[rR][a-zA-Z]*)`), "caution", "deletes files and folders without the Trash"},
	{regexp.MustCompile(`\b(curl|wget|fetch)\b[^|;&]*\|\s*(sudo\s+)?(ba|z|fi|da|k)?sh\b`), "danger", "downloads a script and runs it straight away"},
	{regexp.MustCompile(`\b(ba|z)?sh\s+<\(\s*(curl|wget)`), "danger", "downloads a script and runs it straight away"},
	{regexp.MustCompile(`base64\s+(-d|--decode|-D)[^|]*\|\s*(ba|z)?sh\b|\beval\s+"?\$\(\s*(echo|printf)[^)]*base64`), "danger", "runs hidden (encoded) commands"},
	{regexp.MustCompile(`(^|[;&|]\s*)sudo\b`), "caution", "runs with administrator rights (sudo)"},
	{regexp.MustCompile(`\b(mkfs|newfs_\w+|diskutil\s+(erase\w*|partitionDisk|zeroDisk|secureErase))\b`), "danger", "erases or reformats a disk"},
	{regexp.MustCompile(`\bdd\b[^;|&]*\bof=/dev/`), "danger", "writes directly to a disk device"},
	{regexp.MustCompile(`>\s*/dev/(r?disk|sd)[a-z0-9]*`), "danger", "writes directly to a disk device"},
	{regexp.MustCompile(`:\(\)\s*\{\s*:\s*\|\s*:\s*&\s*\}\s*;\s*:`), "danger", "a fork bomb (freezes the computer)"},
	{regexp.MustCompile(`\bchmod\s+(-R\s+)?[0-7]*777\b`), "caution", "makes files writable by everyone"},
	{regexp.MustCompile(`\bchown\s+-R\b`), "caution", "changes the owner of many files"},
	{regexp.MustCompile(`\bgit\s+push\b[^;|&]*(\s--force\b|\s-f\b|\s\+)`), "caution", "force-pushes, which can overwrite others' work"},
	{regexp.MustCompile(`\bgit\s+(reset\s+--hard|clean\s+-[a-zA-Z]*f)`), "caution", "throws away uncommitted changes"},
	{regexp.MustCompile(`~/\.ssh|\$HOME/\.ssh|/\.ssh/|\bid_(rsa|ed25519|ecdsa)\b|authorized_keys`), "danger", "touches your SSH keys"},
	{regexp.MustCompile(`~/\.aws|\.config/gh/|\.netrc\b|\.npmrc\b|\.docker/config\.json|keys\.(enc|secret)\b`), "danger", "touches stored passwords or tokens"},
	{regexp.MustCompile(`\b(security\s+(find|dump|export)-|dscl\s+\.\s+-passwd|launchctl\s+(load|bootstrap)|crontab\s+-[re]?\b)`), "caution", "changes system or login settings, or reads saved passwords"},
	{regexp.MustCompile(`\b(shutdown|reboot|halt)\b|\bkill(all)?\s+-9\s+(-1|1)\b`), "caution", "shuts down, restarts, or stops everything"},
	{regexp.MustCompile(`\b(nc|ncat|netcat)\b[^;|&]*\s-e\s|/dev/tcp/`), "danger", "opens a remote shell on your computer"},
	{regexp.MustCompile(`\bcsrutil\s+disable|\bspctl\s+--master-disable|\bxattr\s+-[a-z]*d[a-z]*\s+com\.apple\.quarantine`), "caution", "turns off a macOS security protection"},
}

// CommandRisk checks a command an AI suggested. fromOutput is the terminal
// text the AI was shown: a command that appears there word for word may
// have been planted for the AI to repeat.
func CommandRisk(command, fromOutput string) Risk {
	c := strings.TrimSpace(command)
	var r Risk
	seen := map[string]bool{}
	for _, rule := range riskRules {
		if rule.re.MatchString(c) && !seen[rule.reason] {
			seen[rule.reason] = true
			r.Reasons = append(r.Reasons, rule.reason)
			if rule.level == "danger" || r.Level == "" {
				r.Level = rule.level
			}
		}
	}
	if len(c) >= 12 && fromOutput != "" && strings.Contains(fromOutput, c) {
		r.Reasons = append(r.Reasons, "it appears word for word in your terminal output, so it may have been put there for the AI to repeat")
		if r.Level == "" {
			r.Level = "caution"
		}
	}
	return r
}
