# Updates

## Check behavior

A stable installed version makes one unauthenticated HTTPS request to GitHub's latest-release API for `AndreasDellrud/beads-tui` when `App::load` starts. The request runs off the UI thread. Only a newer stable semantic version produces a notice; equal or older releases leave browsing unchanged. Prerelease releases are ineligible. An installed version containing a prerelease suffix or build metadata disables checks, so development builds are not compared against stable releases.

Set `BTUI_UPDATE_CHECK=0` in the environment before starting `btui` to disable requests, including manual rechecks. The setting is inherited by the process; changing it requires restarting `btui`.

There is no persistent cache, scheduled polling, background service, authentication token, or automatic retry. Each process starts fresh. Requests have a five-second total timeout, a 64 KiB response-body cap, and do not follow redirects. GitHub receives the normal network request and btui's user agent; no Beads issue or workspace data is sent.

## Interaction and failures

When a newer stable release is found, a small header notice appears on the browser and issue screens without replacing errors or work feedback. Press `u` to open update information at any time outside filter typing. The overlay shows the installed version, available version and release URL when found, or the disabled, checking, up-to-date, or failure state.

Within the overlay, `r` requests another check. Attempts must be at least 15 minutes apart, including failed attempts; requests while disabled, already checking, or cooling down do nothing. The overlay shows the remaining cooldown. `u` or Escape closes it without hiding the available-update notice. Arrow keys, `j`/`k`, page keys, and the mouse wheel scroll longer information. Other browsing and launch input is captured until the overlay closes. `q` retains the normal pending-launch quit guard.

Offline access, timeout, rate limits, malformed metadata, and other check failures remain visible in the overlay; they do not replace the last Beads view, interrupt navigation, or trigger installation. A successful check finding no newer eligible version also covers an installed version newer than GitHub's latest stable release.

## Update manually

Use the installation method that originally installed your executable. `btui` cannot reliably infer that method and offers instructions rather than running commands or opening a browser. Review the release notes at the displayed release URL, or [GitHub Releases](https://github.com/AndreasDellrud/beads-tui/releases/latest).

- **Prebuilt binary:** download the archive for your supported platform and its checksum. Verify it with `sha256sum --check`, extract it, exit `btui`, and replace the executable at the path you originally installed. The [README installation example](../README.md#prebuilt-linux-binary) uses `~/.local/bin/btui`; preserve your existing location and rollback copy where applicable.
- **Source:** obtain the desired release checkout, review the source, and run `cargo install --path . --locked` from that checkout. Rebuilding an old checkout does not update its source.
- **Package manager:** update through the same package manager or packaging source used for installation. Its available version may lag GitHub; do not replace its owned executable manually.

There is no download, install, self-replacement, or dependency-upgrade action in the UI. See [releasing](releasing.md) for maintainer publication rather than user installation.
