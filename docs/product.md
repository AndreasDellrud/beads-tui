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

The interaction should feel keyboard-native and visually deliberate, not like command output placed inside a border.

## Deliberate exclusions

The first release does not edit, create, claim, close, or synchronize issues. It does not read Beads' Dolt database or passive JSONL export. Mutation can be considered later only with explicit confirmation UX and a clear advantage over invoking `bd` directly.

Live scope and sequencing belong in Beads, not this page.
