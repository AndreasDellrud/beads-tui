# Beads 1.3 migration design

Status: baseline CLI qualification complete; event invalidation implemented and locally validated. Broader release acceptance and selective replay remain open. The owner authorized and completed the live workspace upgrade to 1.3.0 on 2026-09-29. Assessed 2026-09-29 against btui source and pinned Beads v1.3.0 sources. Implementation status belongs in Beads: epic `btui-jdn`, sequenced through `btui-jdn.1`–`btui-jdn.5` (qualification, monitor, selective updates, acceptance, documentation).

## Recommendation

Retain the installed, read-only CLI as the baseline and add the events journal as an optional acceleration. Qualify ordinary 1.3.0 support independently of event support. Preserve the existing polling path for older binaries and journal-disabled workspaces. Defer HTTP: it introduces server lifecycle and connection configuration without being necessary for this desktop TUI.

Target explicit regression coverage for 1.2.2 (the previously installed binary) and 1.3.0. Older versions that satisfy existing command contracts should continue to work through fallback, but are not thereby certified. This is btui compatibility across separate workspaces, not permission to downgrade bd against a database migrated by 1.3.0. Perform binary qualification in separate disposable workspaces; do not upgrade the project database during implementation tests.

## Evidence and suggested changes

[GitHub issue #6](https://github.com/AndreasDellrud/beads-tui/issues/6#issue-5603791519) correctly identifies the two-second `vc status` monitor. Current code already avoids listing when the revision is unchanged; it does not repeatedly diff full snapshots. The opportunity is fewer subprocess launches and more selective updates when work changes.

| Suggestion or contract | Assessment |
| --- | --- |
| Use `events tail --follow` | Adopt optionally. It emits ordered JSONL and internally polls once per second; describe it as a persistent feed, not instantaneous push. |
| Apply full issue state directly | Adopt selectively. A journal snapshot contains the row, labels and `is_blocked`, not enriched dependency/comment collections. Deserializing it into today's `Issue` would default missing enrichment to empty/zero. |
| Missing `is_blocked` means false | Respect this for journal snapshots. Existing `Issue` has no such field and launch eligibility checks status only; qualify list/show availability before extending eligibility, retaining old status checks. Readiness still belongs to bd. |
| Stop all snapshot polling | Unsafe as a universal policy: sync and direct library writes are outside complete journal coverage. Keep reconciliation. |
| Use `bd serve` | Defer. It is preview, needs server/proxied-server storage, and rejects `--readonly serve`. The release notes call embedded refusal permanent, contrary to the issue's temporary wording. Do not plan around a promised embedded fix. |
| New cycles/config/search shapes | No direct impact: btui does not call these commands. Its text filtering is local. |
| New `list --ready` filter restrictions | Current options use none of the newly refused filters. Test every actual view/sort combination rather than broadening flags. |

Initial planning inspected source only. Subsequent embedded CLI qualification passed the command checks below; broader migration and event acceptance remain outstanding.

## Initial qualification evidence

On 2026-09-29, `scripts/qualify-bd` exercised separate fresh embedded stores with
`bd 1.2.2 (6c124203e)` and `bd 1.3.0 (f45b249ce)`. The 1.3.0 Linux amd64
archive was checked against the release checksum
`2f92b904ecf35b607e44dc5c39229173af69c54f1183e8d709f1773540cdcf3b`.
The [1.2.2 captures](../tests/fixtures/bd-1.2.2.json) and
[1.3.0 captures](../tests/fixtures/bd-1.3.0.json) include binary SHA-256,
exact arguments, stdout, stderr and exit status. They contain synthetic fixture
data only. Rust adapter tests replay these captures and check command parity.

All nine view/sort combinations, enriched show with both relationship directions
and a comment, revision status, nested-directory discovery and missing-ID errors
passed. Both releases omit `is_blocked` from the captured list/show responses even
for a dependency-blocked open issue; no launch-eligibility change follows from
these captures. The 1.3.0 show response adds a string `revision` that the existing
adapter safely ignores. Ordering remains bd-owned.

The 1.2.2 events command is unsupported. On 1.3.0, a disabled journal exits zero
with a stderr notice; after explicit fixture-only enablement, readonly tail returns
the expected update. A separate manual process probe held a readonly embedded
follower open while another bd process updated an issue: the write succeeded and
the follower emitted it. This is initial feasibility evidence, not sustained
concurrency or recovery acceptance. A migration-freeze marker did not refuse a
readonly list, so that probe does not qualify migration-refusal handling.

Reproduce captures with a trusted downloaded binary (no installation required):

```bash
./scripts/qualify-bd /absolute/path/to/bd 1.3.0 /tmp/new-bd-1.3.0-capture.json
```

The harness requires Python 3 and Git, refuses existing output, checks the binary
version, isolates home/config and workspace-routing environment, and removes its
disposable workspace. It does not commit, configure remotes, or sync. Its captured
contract is deliberately checked against adapter argument construction by Rust
tests, so changes on either side must be reconciled.

Linked-worktree discovery subsequently passed on both binaries using disposable
unborn Git worktrees. A readonly 1.3.0 read of a copied v53 store refused with an
actionable v53/v66 schema mismatch; the adapter preserves this stderr diagnostic. Server/proxied-mode coverage and live-terminal
acceptance belong to the later matrix (`btui-jdn.4`). No selective event replay is qualified
by these baseline checks, and no broader version/backend support claim
is made from these embedded command checks alone.

## Delivery boundaries

First qualify the existing `--readonly list --json --limit 0` commands (active, ready, closed; three sorts), enriched `show`, and `vc status` on both versions. Keep cwd-based workspace discovery and direct shell-free invocation. Cover structured/ordinary failures, migration refusal and the existing optional-field defaults. Do not adopt `--brief`: the browser uses full text for preview and filtering.

Then introduce one supervised event reader in `src/bd.rs`, with interaction and recovery state in `src/app.rs` and any status rendering in `src/ui.rs`. Keep `src/work.rs` responsible for launch eligibility and foreground lifecycle. Extend the existing source/runner seams only as needed for streaming tests; avoid a general transport framework.

Use read-only capability probes, not a version-number assumption alone. A supported command and an enabled journal are different facts. The CLI can exit successfully with old records or no records while warning on stderr that the journal is disabled; the HTTP-only 409 contract cannot be assumed for the CLI. Resolve effective enablement through a supported bd interface and the pinned disabled-notice contract, conservatively retaining polling when uncertain. Probe failures must not prevent baseline browsing. Do not enable the journal, set writer environment variables, migrate storage, or start a server from btui. Owners may opt in with `bd config set events-journal true`; documentation must explain that all writers need coverage.

The implemented first event stage treats records as invalidations and coalesces bursts into existing background refreshes. This establishes lifecycle and correctness before selective row replacement. Retain current revision monitoring in this stage, plus a bounded full-refresh safety interval for changes that do not move the monitored revision. The initial full reconciliation interval is 30 seconds; performance tuning remains part of the acceptance matrix.

The selective-update stage may lower revision-check frequency after measurement, but must retain bounded full reconciliation and manual refresh. Never infer that a particular revision change is fully represented by nearby journal records. Syncs, journal disable/re-enable gaps, and unjournaled writers can coincide with normal events.

## Stream, snapshot and cache consistency

Keep cursors in memory, scoped to the current resolved workspace and monitor generation. btui persists no mirror, so persisting a cursor alone would complicate startup without avoiding a baseline read. Restarting btui starts a fresh baseline; a changed workspace/clone, restored journal, or regressed cursor forces rebaselining.

Use `bd --readonly events tail --since N --follow --json`, subject to pinned-binary qualification. For future delta replay, advance the applied checkpoint only after the app has successfully accepted the record or its authoritative refresh. The current invalidation-only monitor uses a volatile consumed cursor and coalesced dirty bit; it never applies journal snapshots and cannot use that cursor as a delta-replay checkpoint. Resume from that checkpoint after a recoverable interruption. Discard duplicates; reject unexplained gaps or unknown operations into reconciliation rather than silently skipping them. Support all seven operations, including comments, and null snapshots on delete or cascading dependency removal.

Snapshot and stream ordering is a release gate for deltas. Establish a pre-snapshot checkpoint, buffer concurrent records, and obtain a post-snapshot boundary before exposing reconciled state. Do not replay historical snapshots over newer list data, and do not obtain only a post-list cursor that can skip concurrent changes. The CLI has no documented head-only success envelope: qualification must establish a bounded checkpoint protocol (including an empty/pruned journal), or keep invalidation mode. If a stable boundary cannot be established under concurrent writes, retain authoritative list refreshes. Never use a new timestamp alone as proof of complete ordering.

Handle initial truncation errors as a complete JSON document (potentially pretty printed and enveloped); mid-follow truncation is a single JSONL error record. Rebuild current state and safely establish a fresh cursor on `events_journal_truncated`; never silently accept a gap using `floor - 1`. While recovering, keep the last good view with visible freshness/error feedback.

Give streams bounded queues and record-size limits, drain stderr concurrently, and coalesce redraws. Overflow, malformed records, unsupported payloads and worker failure trigger reconciliation/fallback. Back off retries; stop and reap the child on shutdown and foreground handoff. Resume through a fresh baseline after the agent returns. A watcher that holds an embedded store open must be tested alongside external writers; lock contention is a reason to retain finite polling, not to force a backend migration.

Represent journal rows separately from enriched `Issue` values. Update only proven scalar/label/readiness fields; mark enrichment dirty where needed. Dependency changes invalidate both relevant endpoints and affected open detail; comments invalidate comment content/counts even without a changed timestamp. Deletes remove the row and invalidate relationships referencing it. Generation checks must cover speculative prefetch too: current prefetch replies can insert directly into the cache, so merely removing cached values is insufficient.

Keep `bd list --ready` authoritative until exact membership parity is demonstrated: `!is_blocked` alone is not the ready predicate. Match active/closed visibility, hidden/default exclusions, sort tie-breaking, local text filters and counts before inserting/reordering rows from events. Use coalesced list reads for cases without enough information. Preserve selected identity and bounded scroll offsets across updates. Never replace a detailed issue with a sparse journal row.

## Acceptance and rollout

Use captured, version-labelled fixtures plus actual pinned binaries in disposable workspaces. Fixture-only success is insufficient to claim support. Exercise 1.2.2 fallback and 1.3.0 with the journal off/on, embedded and supported server modes where available. Record untested modes explicitly.

Cover create/update/claim/reopen/close/delete, labels, comments, dependency add/remove and cascades, derived blocked-state changes, concurrent startup/refresh, rapid view changes, stale prefetch, duplicates, pruning, stream death and disabled journals. Include external sync, clone/worktree isolation, empty workspaces, and non-journaled updates to prove reconciliation. A release candidate must pass `./scripts/validate`, `git diff --check`, and live-terminal smoke for selection, navigation, agent handoff, terminal restoration and child cleanup.

Compare idle subprocess count, list/show calls during bursts, refresh latency and responsiveness with current polling. Ship ordinary 1.3.0 compatibility first if selective replay is not ready. Update README and architecture only to describe verified behavior. HTTP, GitHub responses, community-tools PRs, releases, pushes and Dolt synchronization are outside this planning change.

## Event-invalidation validation

The quality gate covers stream command construction, seven operations, duplicates,
gaps, initial/midstream truncation framing, malformed/incomplete/oversized input,
unsupported/disabled fallbacks, stderr pressure, backoff and process reaping.
Application tests cover burst coalescing, stale prefetch rejection, timestamp-
independent detail refresh, retained relationship selection and error visibility
on both screens. A single dirty bit replaces an event queue, so overload coalesces
into a full authoritative refresh instead of accumulating records.

An automated real-PTY smoke against the disposable 1.3.0 embedded workspace
verified a journaled title update in open detail, visible watcher loss, automatic
reconciliation of an unjournaled write, view navigation, foreground handoff to a
fixture executor with no event child present, watcher restart on return, normal
exit, terminal-mode restoration and no surviving watcher. Separate real-PTY
checks verified 1.2.2 fallback and disabled-journal 1.3.0 browsing without spurious
feed errors. This is automated terminal evidence; owner visual acceptance,
server/proxied modes, comprehensive sync/pruning scenarios and performance
measurements remain in `btui-jdn.4`.

The owner subsequently verified title updates with automatic commits disabled:
the journal-disabled path appeared after waiting for reconciliation, and the
journal-enabled path refreshed while `bd vc status` retained the same commit.
This accepts those manual refresh paths, not the entire recovery matrix.

The 0.4.0 release scope is optional event-triggered authoritative refresh plus
the qualified legacy fallback. It does not claim reduced CLI work or direct
application of event snapshots. Selective deltas remain in `btui-jdn.3`, the
broader qualification matrix in `btui-jdn.4`, and the future JSON-envelope
transition in `btui-b2l`.

Local 0.4.0 release preparation passed `scripts/validate` (71 tests, formatting
and lint) on Rust 1.98.1 and the CI minimum Rust 1.88.0. Cargo package verification
passed on both toolchains with `--locked --allow-dirty` for the uncommitted
candidate. The Linux x86-64 archive built with Rust 1.88.0, its SHA-256 verified,
and its extracted binary passed the real-PTY event, fallback, foreground handoff,
terminal restoration and watcher-cleanup checks described above.

The owner delegated final terminal acceptance on 2026-09-29. The extracted
candidate passed real-PTY checks at 140×34 and 80×24 for browsing, filtering,
enriched details, relationship navigation, active/closed views, sorting, refresh
and resizing between the layouts. Rendered screen captures were inspected.
Both sizes also passed visible missing-workspace error handling and retry, with
terminal attributes, alternate-screen exit and mouse-capture release verified
after normal and error-screen exit. This is delegated automated acceptance, not
a claim that the owner personally inspected both layouts. The publication
workflow remains governed by [the release procedure](releasing.md).

## Live workspace upgrade

The owner authorized the live upgrade on 2026-09-29. The user-managed `bd` now
resolves to the versioned 1.3.0 payload. Before explicit local migration, the
workspace monitor was paused, the full `.beads` directory and 1.2.2 binary were
preserved outside the repository, and issue-complete exports were captured.
The migration was run as the designated local migrator without remote sync.
Post-migration comparison preserved all 50 issue identities and fields,
dependency edges, and comment content/authors/timestamps. Comment IDs were
restamped and some collection ordering changed. Status counts also matched.
The live journal remains at its existing disabled setting.

Other workspaces were not migrated. Readonly clients using the newly installed
binary may report schema mismatch until their owners explicitly upgrade them.
Rollback requires the paired pre-migration store and binary, not running 1.2.2
against the upgraded database. Machine-local backup paths are recorded in the
implementation handoff, not as project configuration.

## Sources

- [Current adapter](../src/bd.rs), [application workers/cache](../src/app.rs), [launch eligibility](../src/work.rs), and [current architecture](architecture.md).
- [Beads v1.3.0 release notes](https://github.com/gastownhall/beads/releases/tag/v1.3.0): CLI changes, server restrictions, migration/skew behavior and known missing-journal-table issue. Missing tables must cause visible fallback, not btui repair.
- [Pinned journal reference](https://github.com/gastownhall/beads/blob/v1.3.0/docs/reference/events-journal.md): coverage, snapshots, enablement, retention and replica scope.
- [Pinned CLI implementation](https://github.com/gastownhall/beads/blob/v1.3.0/cmd/bd/events.go): disabled notice, follow cadence and initial/midstream truncation framing.
- [Pinned event envelope](https://github.com/gastownhall/beads/blob/v1.3.0/internal/eventsjournal/record.go).
- [HTTP contract identified by upstream](https://github.com/gastownhall/beads/blob/v1.3.0/internal/httpapi/spec/openapi.v0.yaml): reference for any future separately scoped HTTP evaluation; no HTTP implementation is planned here.
