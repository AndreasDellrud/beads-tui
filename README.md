# beads-tui

[![CI](https://github.com/AndreasDellrud/beads-tui/actions/workflows/ci.yml/badge.svg)](https://github.com/AndreasDellrud/beads-tui/actions/workflows/ci.yml)

`btui` is a human-friendly terminal browser and work-session launcher for [Beads](https://github.com/gastownhall/beads). It turns the structured output of `bd list` and `bd show` into a fast two-pane view without becoming a second task database or mutating Beads itself.

The first slice supports issue navigation, detail viewing, active/ready/closed views, useful filtering, sorting, automatic external-change refresh, responsive narrow-terminal layout, and status/priority styling.

## Prerequisites

- Rust 1.88 or newer
- `bd` on `PATH`
- an active Beads workspace in the current directory or one of its parents
- `codex` and/or `claude` on `PATH` to start implementation sessions
- optionally, [Herdr](https://herdr.dev) for managed agent worktrees

## Run

```bash
cargo run --release
```

Keys: `↑`/`↓` or `j`/`k` navigate; `Enter` opens the dedicated issue screen; `1`, `2`, and `3` switch active, ready, and closed views; `s` cycles priority, updated, and created sorting; `/` filters; `x` clears the filter; `w` starts work with the active agent; `a`/`A` shift forward/backward through installed agents; `r` refreshes in the background; and `q` quits. The mouse wheel scrolls whichever pane is under the pointer without changing the selected issue. `Page Up` and `Page Down` scroll the browser preview. In the issue screen, `↑`/`↓`, `j`/`k`, and page keys scroll the bounded issue body; `Tab`/`Shift-Tab` select relationships, `Enter` follows one, and `Esc` or `Backspace` returns to the browser. The active agent remains visible in both screens' footer.

`w` passes only the task ID and repository path to a shared prompt that tells the selected agent to read `AGENTS.md`, retrieve the canonical Beads task and relationships, confirm readiness, claim it, create a focused branch, implement it, validate it, and update Beads. Closed, blocked, and deferred tasks are rejected before launch. When `btui` is running inside Herdr, the action creates a branch-backed worktree workspace, starts the selected Codex or Claude Code agent there, and focuses it, so its branch cannot replace the branch under the TUI. If an agent requires startup confirmation, `btui` focuses that screen and continues the original launch after approval; it never bypasses trust. Elsewhere it temporarily restores the terminal, runs the selected interactive agent in the foreground, then restores `btui` and refreshes Beads when the agent exits. `Shift-W` always requests the foreground path and is the explicit retry when managed launch fails.

The installed-agent order is Codex then Claude Code. The active selection is saved immediately in `$XDG_CONFIG_HOME/btui/config.toml`, or `~/.config/btui/config.toml` when `XDG_CONFIG_HOME` is unset. A missing, malformed, or unavailable saved choice falls back deterministically to the first installed supported agent and reports the fallback. This selection is owned by `btui`; it does not depend on Omarchy or a desktop-level default-agent setting.

Executable detection does not imply that an agent is authenticated. A first-run login or trust screen stays visible in the preserved foreground or Herdr session for the user to complete. Managed prompt submission waits briefly for observed activity; if the agent remains at setup instead of beginning work, btui reports the launch problem without deleting that session.

Launch feedback appears on a dedicated status row, leaving the keybinding footer visible. A successful launch message clears on the next keypress or mouse-wheel scroll. Launch errors remain available for inspection and retry until dismissed with `Esc`.

While it is open, `btui` checks Beads' supported version-control revision in a background worker. A changed revision triggers one quiet list refresh, so changes made by another shell or agent appear without repeatedly running the heavier list command. If revision detection fails, the status line reports that automatic refresh is paused; manual `r` refresh remains available while later checks retry automatically.

## Install

Install from a source checkout with Cargo:

```bash
cargo install --path . --locked
btui
```

This installs the `btui` executable into Cargo's binary directory, normally `~/.cargo/bin`. Remove it with `cargo uninstall beads-tui`.

The manually dispatched **Release** GitHub Actions workflow creates a matching version tag and GitHub Release with generated notes, a Linux x86-64 archive, and its SHA-256 checksum. Published versions are available from the [GitHub releases page](https://github.com/AndreasDellrud/beads-tui/releases). After downloading an archive and checksum:

```bash
sha256sum --check btui-*.tar.gz.sha256
tar -xzf btui-*.tar.gz
install -Dm755 btui-*/btui ~/.local/bin/btui
```

## Validate

```bash
./scripts/validate
```

The application always invokes `bd` itself with `--readonly`. Beads remains the source of truth; this project does not read Dolt internals or `.beads/issues.jsonl`. Any task mutation happens later inside the user-visible agent session, not as a partial side effect of pressing `w`.

GitHub Actions runs the same validation script on pushes and pull requests. Release preparation and the intentionally manual publication flow are documented in [docs/releasing.md](docs/releasing.md).

Project intent and boundaries are maintained in [docs/product.md](docs/product.md) and [docs/architecture.md](docs/architecture.md). Live work is tracked only in Beads (`bd ready`, `bd list`).
