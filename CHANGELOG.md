# Changelog

All notable changes to `btui` are recorded here.

## Unreleased

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
