# Product

## Purpose

Beads is efficient from the command line, but scanning several issues through repeated `bd list` and `bd show` calls asks people to mentally assemble context. `btui` provides a calm, information-dense browser for that exploration while retaining `bd` as the sole authority.

## First release

The initial experience is a read-only task browser:

- list every non-closed issue returned by `bd list`
- scan an instant detail preview built from the selected `bd list` record
- open a dedicated issue screen for the richer `bd show` representation, including comments and navigable relationships
- make status, priority, identity, ownership, dependencies, and acceptance criteria easy to scan
- filter the loaded list by issue ID or title
- refresh automatically after another process changes Beads, retain explicit refresh, and recover with an actionable error when `bd` or its workspace is unavailable
- remain useful in both wide and narrow terminals
- keep selection independent from pane scrolling, with bounded overflow in lists, previews, issue bodies, and relationships

Mouse navigation complements the keyboard: a single click selects an issue or relationship, a double click opens it, and visible footer controls invoke the same actions as their keys. Wheel scrolling remains independent of selection.

The interaction should feel keyboard-native and visually deliberate, not like command output placed inside a border.

## Deliberate exclusions

The first release does not edit, create, claim, close, or synchronize issues. It does not read Beads' Dolt database or passive JSONL export.

## Work sessions

The next slice adds one deliberate control action without turning `btui` into an issue editor:

- start an implementation session for the selected eligible bead with `w`
- detect installed Codex and Claude Code executors, show the active choice, and shift the persisted default with `a`/`A`
- keep Beads canonical by prompting with the bead ID rather than a copied description
- require the agent to read repository instructions, inspect readiness and dependencies, claim the bead, create a branch, validate its work, and update Beads
- create a branch-backed managed worktree workspace using the selected agent kind when Herdr capability is present, with the selected interactive agent in the foreground everywhere else
- preserve and focus startup confirmation screens, then continue the original managed launch after the user approves them
- show transient success and persistent dismissible errors above a permanently visible keybinding footer
- restore terminal state and refresh Beads after a foreground session returns
- reject closed, blocked, and deferred beads before starting and make launch failures visible and retryable

`btui` still performs no Beads mutation itself. It does not pre-claim a task. Foreground branch creation remains the agent's responsibility; Herdr creates a branch as part of the isolated worktree operation so the checkout running `btui` is never switched. It never bypasses agent trust: confirmation remains an explicit user decision, and a live blocked or timed-out Herdr session is preserved rather than silently deleted. Executor-specific process control stays behind a task-to-session boundary so another adapter can be added without changing selection or lifecycle policy. The selected default belongs to `btui` and does not depend on Omarchy. Shell is deliberately excluded until it can satisfy the same prompt and lifecycle contract.

Live scope and sequencing belong in Beads, not this page.
