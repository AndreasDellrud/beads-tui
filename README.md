# beads-tui

[![CI](https://github.com/AndreasDellrud/beads-tui/actions/workflows/ci.yml/badge.svg)](https://github.com/AndreasDellrud/beads-tui/actions/workflows/ci.yml)

`btui` is a human-friendly, read-only terminal browser for [Beads](https://github.com/gastownhall/beads). It turns the structured output of `bd list` and `bd show` into a fast two-pane view without becoming a second task database.

The first slice supports issue navigation, detail viewing, active/ready/closed views, useful filtering, sorting, automatic external-change refresh, responsive narrow-terminal layout, and status/priority styling.

## Prerequisites

- Rust 1.88 or newer
- `bd` on `PATH`
- an active Beads workspace in the current directory or one of its parents

## Run

```bash
cargo run --release
```

Keys: `↑`/`↓` or `j`/`k` navigate; `Enter` opens the dedicated issue screen; `1`, `2`, and `3` switch active, ready, and closed views; `s` cycles priority, updated, and created sorting; `/` filters; `c` clears the filter; `r` refreshes in the background; and `q` quits. The mouse wheel scrolls whichever pane is under the pointer without changing the selected issue. `Page Up` and `Page Down` scroll the browser preview. In the issue screen, `↑`/`↓`, `j`/`k`, and page keys scroll the bounded issue body; `Tab`/`Shift-Tab` select relationships, `Enter` follows one, and `Esc` or `Backspace` returns to the browser.

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

The application always invokes `bd` with `--readonly`. Beads remains the source of truth; this project does not read Dolt internals or `.beads/issues.jsonl`.

GitHub Actions runs the same validation script on pushes and pull requests. Release preparation and the intentionally manual publication flow are documented in [docs/releasing.md](docs/releasing.md).

Project intent and boundaries are maintained in [docs/product.md](docs/product.md) and [docs/architecture.md](docs/architecture.md). Live work is tracked only in Beads (`bd ready`, `bd list`).
