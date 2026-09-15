# Releasing

## Supported paths

The source installation path is:

```bash
cargo install --path . --locked
```

GitHub Actions owns repeatable CI and release construction:

- **CI** runs `./scripts/validate` and verifies `cargo package --locked` on pushes, pull requests, and manual dispatches.
- **Release** is manually dispatched from `main` with the intended Cargo version. It validates the request, builds the locked Linux x86-64 archive and checksum, retains them as a workflow artifact for 14 days, creates the matching `v<version>` tag, generates release notes, and publishes both files on a GitHub Release.

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
5. Merge the release preparation pull request into protected `main` after CI passes.
6. Open **Actions → Release → Run workflow**, select `main`, enter the version without `v`, and run it.
7. Verify the resulting tag, generated notes, archive, and checksum from the public Releases page.

The workflow rejects a non-`main` dispatch, a version that differs from `Cargo.toml`, or an existing tag or release. Runs for the same requested version cannot overlap. GitHub retains one active run and at most the newest pending duplicate for that version; dispatching another duplicate cancels and replaces the older pending run. After the active run publishes, the surviving pending run fails the existing tag or release check instead of replacing assets. This is intentional because requests for the same version are redundant. The workflow's write permission is scoped to the release workflow; ordinary CI retains read-only repository permissions.

## Reruns and failures

Failures before the final publication step are safe to rerun with the same input. Do not blindly rerun after **Create tag and publish GitHub Release** begins: first inspect the repository's Releases and Tags pages. If an incomplete draft, release, or tag exists, remove only that incomplete release state before retrying. A completed release is immutable for this workflow; it deliberately refuses to overwrite or move it.

The workflow does not publish to crates.io or modify Beads state. Those remain separate owner decisions.

The public repository protects `main`. Release preparation and ordinary changes must merge through pull requests with passing GitHub Actions rather than direct pushes. Manually dispatching **Release** from `main` is the explicit publication decision.

Before closing a release, verify that `btui` restores the alternate screen and mouse-capture state after both normal use and a visible Beads workspace error. Wide and narrow layouts require explicit owner acceptance.
