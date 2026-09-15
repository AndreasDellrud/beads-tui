# Architecture

## Current implementation

`btui` is a single Rust process with three small boundaries:

1. `src/bd.rs` invokes `bd --readonly` directly, without a shell, and deserializes JSON from `bd list` and `bd show`. Its narrow command-runner seam keeps process outcomes deterministic in tests without changing the production API.
2. `src/app.rs` owns views, sorting, semantic selection, independent bounded viewport offsets, filtering, background refresh, speculative relationship cache warming, and recoverable error state. A dedicated standard-library worker translates active/ready/closed views and priority/updated/created sorting into `bd list` options; generation IDs prevent superseded refresh results from replacing newer state. A second worker checks the supported `bd vc status --json` revision every two seconds and requests a quiet list refresh only when that revision changes. Detection failures are visible, retried, and do not disable manual refresh. Selection renders immediately from the list record and retains its issue identity across refreshes where possible. A separate two-worker queue runs speculative `bd show` requests so the selected relationship and its nearest neighbors warm concurrently without delaying authoritative list or foreground detail work. IDs are deduplicated, a newly visible issue replaces obsolete pending work, and changing relationship selection reprioritizes that target.
3. `src/ui.rs` renders that state with Ratatui. It uses side-by-side panes in wide terminals and stacked panes in narrow terminals, calculates content-aware scroll bounds after wrapping, and maps mouse coordinates to the pane under the pointer. Wheel scrolling therefore moves a viewport without changing issue or relationship selection.

The process inherits its working directory so Beads resolves the same workspace a direct `bd` command would. It owns no issue persistence and calls no mutating command.

```text
keyboard -> application state -> Ratatui frame
                    |
                    +-> bd --readonly list/show --json -> typed issue data
```

## Authority and compatibility

The installed `bd` executable and its JSON output are authoritative. The typed representation deliberately defaults optional display fields so additions and omitted optional values remain compatible; missing identity, title, status, priority, or type is treated as malformed output.

Subprocess failures retain `bd`'s diagnostic text. The human-oriented ANSI stream from `bd list --watch` is deliberately not parsed as an API; its installed implementation is itself a two-second polling display. Automatic invalidation instead uses the typed version-control status JSON, and the existing typed list adapter remains the only source of list records. Browser preview fields come directly from the authoritative list snapshot, so navigation never pays for redundant `bd show` processes. Opening the dedicated issue screen requests comments and dependents through the generation-guarded primary worker, caches the enriched issue by identity and freshness, and ignores results after the user returns to the browser or follows another relationship. Once relationships are known, their extended representations warm into that same cache in selection-proximity order without replacing the visible issue. A failed speculative request stays silent and normal foreground navigation remains the retry path. Warming is deliberately scoped to relationships rather than unrelated repository history.

## Validation boundary

Unit tests cover representative extended JSON fixtures, exact view/sort/show/revision command construction, process failures, JSON adaptation, non-blocking delayed loads, zero-show browser navigation, revision-triggered refresh, unchanged-revision suppression, transient monitor recovery, refresh and detail generations, relationship warming order, deduplication and concurrency bounds, cached screen reopening, related-Bead navigation, selection retention, bounded pane scrolling, pointer-to-pane routing in wide and narrow layouts, multi-field filtering, and recoverable worker errors. `cargo clippy` and `cargo fmt` protect the Rust boundary. A live smoke test still requires a terminal and an active Beads workspace; this is human acceptance rather than something the validation script silently pretends to exercise.
