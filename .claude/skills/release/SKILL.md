---
name: release
description: Publish a jamalterm version (stable or Beta) and confirm people can actually get it. Use when asked to release, publish, ship a version, or when the user says an update isn't showing.
---

# Releasing jamalterm

4.x (Rust): the publish job, the Sigstore proof and the updater are back (docs/DESIGN.md §0). The proof is checked in CI by `cargo test verify_release`, the same code the app uses.

A release isn't done until it's on GitHub's Releases page with its build proof. Never tell the user a version is out before step 5 says so.

## 1. Ready?
- `CHANGELOG.md` has a `## X.Y.Z` section for this version (it becomes the release notes and the app's What's new).
- The latest push to `main` is green: `mcp__github__actions_list` (`list_workflow_runs`, `build.yml`, `per_page: 1`). All jobs count: `linux`, `macos`, both `shells` jobs. If red, read the failing job's log with `mcp__github__get_job_logs` (`tail_lines` ~60), fix it, push, and wait again.

## 2. Publish
Either:
- push a commit titled exactly `Release X.Y.Z` (`Release X.Y.Z-beta.N` for the Beta channel). Its own build publishes when every job passes, with no second build; or
- dispatch `build.yml` on `main` with `inputs: {"version": "X.Y.Z"}`. It builds `main` as it is at that moment, so don't push anything else until the run has been created.

## 3. Wait without guessing
A run takes about 7–10 minutes. Wait in the background (`sleep 240` with `run_in_background`), then check the run. Don't poll in a tight loop, and don't announce anything while it runs.

## 4. If publishing fails
Read the log first. Don't just retry.
- **`HTTP 403: Resource not accessible by integration`** on `gh release create`: the workflow token can't write.
  1. Repo admin `actions-check` for `jamalterm` (see the github-autopilot skill), then read `actions-jamalterm.txt` in `jamalxcode/repo-audit`.
  2. If the token default is `read`: Repo admin `actions-allow-write`, or ask the user to set Settings → Actions → General → Workflow permissions to *Read and write*.
  3. Then publish the run that already built and passed: Repo admin `release` with `run_id` and `version`. Nothing is rebuilt, so the Sigstore proof still matches. Re-running the failed job did **not** pick up the new setting (2026-11-10).
- **Tag already exists**: the version was used before. Bump it; never move or delete a published tag.
- **Anything else in tests or the build**: fix it on `main`, then release from a new green run.

This session can't push tags or create releases directly (the proxy drops them). Don't try `git push --tags`.

## 5. Confirm
- `mcp__github__get_latest_release` shows `vX.Y.Z` (for a Beta, `list_releases` shows it as a prerelease).
- Its assets: `jamalterm-X.Y.Z.dmg`, `jamalterm-X.Y.Z-macos.zip`, `jamalterm-X.Y.Z-macos.zip.sigstore.json`, `SHA256SUMS.txt`. Without the `.sigstore.json`, the updater refuses to install it.
- Then tell the user: the version, the release link, and that the app offers it on its next check (at launch, then every 6 hours, or right away from Settings → Updates → Check Now).
