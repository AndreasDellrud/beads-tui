# Changelog

All notable changes to `btui` are recorded here.

## Unreleased

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
