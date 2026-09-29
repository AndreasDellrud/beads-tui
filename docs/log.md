# Documentation log

## 2026-09-29

- Passed owner-delegated final 0.4.0 terminal acceptance at 140×34 and 80×24, including resize/navigation and terminal/mouse restoration after normal and missing-workspace-error exits.

- Prepared the 0.4.0 event-assisted refresh release and recorded owner verification of journaled and unjournaled title updates with an unchanged version-control revision. Direct event deltas, broader backend/performance qualification and JSON envelopes remain Beads follow-up work.

- Implemented optional readonly event invalidation with polling fallback, bounded stream handling, child cleanup, periodic reconciliation and generation-safe detail invalidation. Recorded verified live 1.3.0 migration; journal enablement and selective replay remain separate.

- Began Beads 1.3 compatibility implementation with a disposable-workspace qualification harness and captured 1.2.2/1.3.0 CLI regression contracts; recorded embedded results and explicit remaining qualification boundaries.

- Assessed upstream issue #6 against pinned Beads v1.3.0 contracts and current btui code; recorded a staged CLI-first migration design with legacy fallback, event reconciliation, cache consistency, and deferred HTTP. Implementation remains tracked in Beads.

## 2026-09-24

- Added issue and relationship click selection, double-click opening, and clickable footer actions with scroll-aware hit testing for wide and stacked layouts.

## 2026-09-16

- Reworked the public README around a visual product preview, benefits, quick installation, scannable controls, the bead-to-agent flow, and explicit safety boundaries; added matching GitHub repository presentation metadata.
- Added a btui-owned Codex/Claude Code selector with installed-executable detection, XDG persistence, visible `a`/`A` shifting, executor-neutral prompts, and generic foreground/Herdr launch adapters without an Omarchy dependency.
- Changed managed launches from Codex-specific tabs to branch-backed Herdr worktree workspaces so every supported executor receives the same checkout-isolation guarantee.

## 2026-09-15

- Separated launch feedback from persistent keybindings; successful Codex status clears on the next interaction while launch errors remain explicitly dismissible.
- Preserved Herdr-managed Codex sessions across first-run hook confirmation and resumed the original task prompt after explicit approval without bypassing hook trust.
- Added executor-neutral task-to-work-session orchestration, canonical bead-ID prompts, safe foreground terminal handoff, and Herdr-managed Codex tabs without adding direct Beads mutations to `btui`.
- Completed the manual release workflow: validate the requested Cargo version from protected `main`, refuse existing tags/releases, retain the build artifact, create the version tag, generate categorized notes, and publish the archive plus checksum on GitHub Releases.
- Recorded protected `main` as the repository workflow: changes merge through pull requests after GitHub Actions passes.
- Recorded the public GitHub repository in Cargo metadata and exposed its passing CI workflow from the README.
- Prepared Cargo installation and Linux binary packaging, added SHA-256 release artifacts, and placed push/pull-request validation plus manual/tagged artifact construction in GitHub Actions without enabling publication.
- Added automatic external-change detection through the supported Beads version-control revision, with quiet selection-preserving list refresh, restrained list execution, and recoverable monitor errors; documented why the ANSI `bd list --watch` display is not used as an adapter.
- Expanded relationship prefetch into bounded two-worker cache warming, ordered around the current selection and scoped to the visible issue's relationship set.
- Added coalesced background prefetch for highlighted relationships so following a warmed relationship opens directly from the detail cache without exposing another `bd show` delay.
- Separated semantic selection from viewport offsets and added bounded, pane-aware wheel scrolling for issue lists, previews, full issue bodies, and relationships.
- Added a dedicated, background-enriched issue screen with comments, metadata, selectable dependency relationships, related-Bead navigation, and preserved return to the browser.
- Added explicit active, ready, and closed views; priority, updated, and created sorting; multi-field text filtering; clear-filter behavior; and visible/loaded result counts.
- Removed redundant `bd show` calls from selection; the complete current detail view is pre-rendered from list data, while list refresh runs on a generation-guarded background worker.
- Hardened the `bd` adapter around an injectable command runner and representative list/show fixtures, including explicit process and schema failure coverage.
- Established the product boundary and current Rust/Ratatui architecture for the first read-only task browser.
- Recorded `bd --readonly` JSON commands as the integration authority and Beads as the only live backlog.
