#!/bin/bash
# Runs jamalterm's self-test on this Mac (CI): the app drives itself through
# its main features; this script presses real keys (if macOS allows it),
# measures memory, and checks the report. Results and screenshots: smoke/.
set -uo pipefail
cd "$(dirname "$0")/.."
OUT="$PWD/smoke"
rm -rf "$OUT"; mkdir -p "$OUT"
APP=build/bin/jamalterm.app
export JAMALTERM_SMOKE="$OUT"

wait_for() { for _ in $(seq 1 "$2"); do [[ -f "$1" ]] && return 0; sleep 0.5; done; return 1; }

T0=$(date +%s)
"$APP/Contents/MacOS/jamalterm" >"$OUT/app.log" 2>&1 &
PID=$!

if wait_for "$OUT/ready-for-keys" 240; then
  # Memory with one window and two panes: the app plus its web views.
  APP_KB=$(ps -o rss= -p "$PID" | tr -d ' ')
  WEB_KB=$(ps -Ao rss=,comm= | awk '/WebKit.WebContent|com.apple.WebKit.WebContent/ {s+=$1} END {print s+0}')
  echo "app_mb=$((APP_KB / 1024)) web_mb=$((WEB_KB / 1024))" > "$OUT/memory.txt"
  # Real shortcuts: ⌘T (new tab) then ⌘D (split). Needs permission to press keys.
  if osascript -e "tell application \"System Events\" to set frontmost of (first process whose unix id is $PID) to true" \
               -e 'delay 0.5' \
               -e 'tell application "System Events" to keystroke "t" using command down' \
               -e 'delay 1' \
               -e 'tell application "System Events" to keystroke "d" using command down' >"$OUT/keys.log" 2>&1; then
    echo ok > "$OUT/keys-done"
  else
    echo skipped > "$OUT/keys-done"
  fi
else
  echo "The app never got ready." >&2
fi

wait_for "$OUT/report.json" 240
kill "$PID" 2>/dev/null
cat "$OUT/app.log" | tail -20

python3 - "$OUT" "$(( $(date +%s) - T0 ))" <<'PY'
import json, os, sys
out, secs = sys.argv[1], sys.argv[2]
try:
    r = json.load(open(os.path.join(out, "report.json")))
except Exception as e:
    print(f"::error::No smoke report: {e}")
    sys.exit(1)
mem = open(os.path.join(out, "memory.txt")).read().strip() if os.path.exists(os.path.join(out, "memory.txt")) else "unknown"
mark = {True: "✅", False: "❌", None: "⏭ skipped"}
lines = ["## Smoke test on macOS", "",
         f"Launch to first prompt: **{r.get('launchMs')} ms** · 200,000 lines of output: **{r.get('seqMs')} ms** · memory: {mem} · windows at the end: {r.get('windows')} · total {secs}s", "",
         "| Step | Result | ms | Detail |", "|---|---|---|---|"]
for s in r["steps"]:
    lines.append(f"| {s['name']} | {mark[s['ok']]} | {s['ms']} | {s.get('detail', '')} |")
boot = r.get("boot") or {}
if boot:
    lines += ["", "Start-up (ms since the page began loading): " + " · ".join(f"{k} {v}" for k, v in boot.items())]
# Loose limits: they catch big slowdowns, not noise.
slow = []
if (r.get("launchMs") or 0) > 10000: slow.append(f"launch took {r['launchMs']} ms (budget 10000)")
if (r.get("seqMs") or 0) > 15000: slow.append(f"200,000 lines took {r['seqMs']} ms (limit 15000)")
# Memory budget, measured with one window and two panes (app + web views).
import re
m = re.search(r"app_mb=(\d+) web_mb=(\d+)", mem)
if m and int(m.group(1)) > 400: slow.append(f"the app uses {m.group(1)} MB (budget 400)")
if m and int(m.group(2)) > 900: slow.append(f"the web views use {m.group(2)} MB (budget 900)")
for m in slow:
    lines.append(f"\n❌ Too slow: {m}")
summary = "\n".join(lines)
print(summary)
if os.environ.get("GITHUB_STEP_SUMMARY"):
    open(os.environ["GITHUB_STEP_SUMMARY"], "a").write(summary + "\n")
sys.exit(0 if r.get("ok") and not slow else 1)
PY
