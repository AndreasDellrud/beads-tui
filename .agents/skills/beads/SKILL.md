---
name: beads
description: Use when working in a repository that uses bd or Beads for durable project task tracking, issue dependencies, blocker management, multi-session handoff, or shared work memory. Trigger when the user asks to find ready work, claim or close tasks, create follow-up work, inspect blockers, recover project context, or choose between local planning and persistent project tracking.
---

# Beads

Use Beads as the shared project task system. Local plans, scratch files, and personal memories are useful, but they are not the durable source of truth for project work.

## First Step

Run:

```bash
bd prime
```

If that prints nothing, check whether the repository has an active Beads workspace:

```bash
bd where
```

## Preferred Route

Use the `bd` CLI when shell access is available. It is the most compact and direct Beads interface.

## Core CLI Workflow

1. Find work with concise discovery responses. Check `bd --version` and the
relevant command's `--help` once per environment; recheck if the binary changes.
Beads 1.3+ supports `--brief` for JSON `ready` and `list` output. When the
installed command advertises it, prefer:

```bash
bd ready --json --brief
bd list --status=open --json --brief
bd list --status=in_progress --json --brief
```

On older binaries or commands without `--brief`, keep using `bd ready` and
`bd list --status=...` (add `--json` when parsing). Do not upgrade a binary or
migrate a workspace just to enable brief discovery. Do not combine `--brief`
with `--claim`, `--gated`, `--mol`, or `--explain`; use the normal command when
those modes are needed. Treat workspace/schema failures as errors, not empty
backlogs or evidence that the brief flag is unsupported.

2. Inspect the complete selected issue before planning or implementation:

```bash
bd show <id> --json
```

Brief rows omit descriptions, design, acceptance criteria, notes and other
free-form fields. Missing fields are not evidence that the task has no
requirements. Fetch the full issue, plus relevant comments and dependencies,
before acting; never implement from the discovery row alone.

3. Claim work atomically:

```bash
bd update <id> --claim
```

4. Create durable follow-up work when implementation reveals new tasks:

```bash
bd create "Short title" --description="Why this exists and what needs to be done" --type=task --priority=2
```

5. Close completed work:

```bash
bd close <id> --reason="Completed"
```

## What Belongs In Beads

Use Beads for:

- shared project tasks
- blockers and dependencies
- discovered follow-up work
- work that must survive thread reset, compaction, or handoff
- status that another person or agent should be able to resume

Use agent-local planning tools only for the current turn's execution checklist. Do not treat them as shared project state.

## Rules

- Do not create markdown TODO files as the source of truth when Beads is available.
- Do not use `bd edit`; it opens an interactive editor. Use `bd update` flags instead.
- Prefer `--json` when parsing `bd` output programmatically.
- If hooks are installed, `bd prime` may already be injected. Run it manually when context is missing.
- Do not auto-close or mutate tasks unless the work is actually complete.
