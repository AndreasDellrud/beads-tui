# Releasing

## Supported paths

The source installation path is:

```bash
cargo install --path . --locked
```

GitHub Actions owns repeatable CI and artifact construction:

- **CI** runs `./scripts/validate` and verifies `cargo package --locked` on pushes, pull requests, and manual dispatches.
- **Build release artifact** runs validation, builds a locked Linux x86-64 binary, packages it with the README and license, generates a SHA-256 checksum, and uploads both to the workflow run for 14 days.

Both workflows use Rust 1.88.0, the declared minimum supported version. The local packaging equivalent is:

```bash
./scripts/package x86_64-unknown-linux-gnu
```

Artifacts are written to `dist/` and are not committed.

## Versioned release preparation

1. Update `version` in `Cargo.toml` and refresh `Cargo.lock`.
2. Move the matching changelog section from `Unreleased` to the release date.
3. Run `./scripts/validate` and `cargo package --locked`.
4. Run `./scripts/package x86_64-unknown-linux-gnu` and verify its checksum.
5. Create a `v<version>` tag only after the release contents are accepted.

The tag-triggered workflow rejects a tag whose name does not match `Cargo.toml`.

## Publication boundary

The prepared workflow creates temporary GitHub Actions artifacts only. It does not publish to crates.io, create a GitHub Release, push tags, or modify Beads state. Those operations require an explicit owner decision after the repository and release policy exist.

Before closing a release, verify that `btui` restores the alternate screen and mouse-capture state after both normal use and a visible Beads workspace error. Wide and narrow layouts require explicit owner acceptance.
