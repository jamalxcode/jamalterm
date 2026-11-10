#!/bin/bash
# Runs jamalterm's self-test on this Mac (CI): the app types into its own
# shell, checks what it sees and writes smoke/report.json; this script adds a
# screenshot and the memory used, and fails the build if a check failed.
set -uo pipefail
cd "$(dirname "$0")/.."
OUT="$PWD/smoke"
rm -rf "$OUT"; mkdir -p "$OUT"
APP=build/bin/jamalterm.app
export JAMALTERM_SMOKE="$OUT" JAMALTERM_SETTINGS="$OUT/settings.json"

T0=$(date +%s)
"$APP/Contents/MacOS/jamalterm" >"$OUT/app.log" 2>&1 &
PID=$!
sleep 3
screencapture -x "$OUT/screen.png" 2>/dev/null || true
KB=$(ps -o rss= -p "$PID" | tr -d ' ')
echo "${KB:-0}" > "$OUT/memory-kb.txt"
for _ in $(seq 1 120); do [[ -f "$OUT/report.json" ]] && break; sleep 0.5; done
kill "$PID" 2>/dev/null
tail -20 "$OUT/app.log"

python3 - "$OUT" "$(( $(date +%s) - T0 ))" <<'PY'
import json, os, sys
out, secs = sys.argv[1], sys.argv[2]
try:
    r = json.load(open(os.path.join(out, "report.json")))
except Exception as e:
    print(f"::error::No smoke report: {e}")
    sys.exit(1)
mb = int(open(os.path.join(out, "memory-kb.txt")).read().strip() or 0) // 1024
lines = ["## Smoke test on macOS (Rust)", "", f"Memory: **{mb} MB** · total {secs}s", "", "| Check | Result | Detail |", "|---|---|---|"]
for c in r["checks"]:
    lines.append(f"| {c['name']} | {'✅' if c['ok'] else '❌'} | {c['detail']} |")
open(os.environ.get("GITHUB_STEP_SUMMARY", "/dev/null"), "a").write("\n".join(lines) + "\n")
print("\n".join(lines))
bad = [c["name"] for c in r["checks"] if not c["ok"]]
# A native app should stay small (the web-view version allowed 1.3 GB).
if mb > 250:
    bad.append(f"memory {mb} MB > 250 MB")
if bad:
    print("::error::Smoke test failed: " + ", ".join(bad))
    sys.exit(1)
PY
