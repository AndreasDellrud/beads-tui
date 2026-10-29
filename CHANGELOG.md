# Changelog

All notable changes to `btui` are recorded here.

## Unreleased

## 0.4.3 - 2026-10-29

- Accept both legacy Beads JSON and schema-1 envelopes when users enable `BD_JSON_ENVELOPE=1`, without overriding their environment. Qualify Beads 1.2.2 legacy and 1.3.0 unset/0/1 modes; future Beads 2.0 remains unqualified.
- Preserve stdout and stderr diagnostics, reject malformed or unsupported schemas and truncated snapshots visibly, and keep the last good view available on failure.
- Handle enveloped initial event-truncation errors while preserving ordinary JSONL and mid-follow error framing, polling fallback and recovery.
- Extract release versions correctly from Cargo package IDs in named worktrees so local archives retain their canonical filenames.

## 0.4.2 - 2026-10-29

- Allow different Herdr tasks to start concurrently while preventing duplicate pending starts of the same task. Keep pending tasks visible beside completion feedback, and let startup errors and warnings be dismissed individually without losing another task's outcome.

## 0.4.1 - 2026-10-29

- Keep navigation, filtering, resizing and refresh responsive while Herdr creates a worktree, starts an AI agent or waits for startup confirmation.
- Show pending task startup and return success, warnings and failures to the TUI. Prevent overlapping launches and wait for startup to finish before normal quit so worktree creation and prompt delivery are not abandoned.

## 0.4.0 - 2026-09-29

- Qualify the read-only CLI adapter against Beads 1.2.2 and 1.3.0, including linked workspaces.
- Refresh from optional Beads 1.3 event notifications while retaining revision polling and 30-second reconciliation for unjournaled writes. Journal enablement remains an explicit workspace-owner choice.
- Reload open details and invalidate prefetched data after external changes, preserving issue and relationship selection even when timestamps do not change.
- Recover from event-feed failures with visible status and polling fallback; stop and reap the watcher during foreground agent handoff and on exit.

## 0.3.0 - 2026-09-24

- Select issues and relationships with a click, and open them with a double click.
- Click visible footer controls to invoke their keyboard actions.
- Match mouse targets to scrolled content in wide and stacked layouts, ignoring borders, blank rows, and clipped controls.

## 0.2.0 - 2026-09-16

- Start a focused implementation session with the selected Codex or Claude Code executor.
- Shift the default installed agent with `a`/`A` and persist the choice in btui's XDG configuration.
- Use branch-backed Herdr worktree workspaces when available, with safe foreground terminal handoff elsewhere.
- Resume the original Herdr launch after an agent startup confirmation instead of requiring a second attempt.
- Keep keybindings visible in a dedicated footer while transient success and dismissible launch errors use a separate status row.
- Keep task lookup, readiness checks, claiming, and branch creation inside the launched agent so Beads remains canonical and launch failures do not partially mutate work.

## 0.1.0 - 2026-09-15

- Browse active, ready, and closed Beads issues with filtering and sorting.
- Preview list-backed issue information without blocking navigation.
- Open extended issue details, comments, dependencies, and dependents.
- Follow relationships using a proactively warmed detail cache.
- Scroll long lists, previews, issue bodies, and relationship panes independently.
- Refresh automatically after external Beads changes while preserving selection.
