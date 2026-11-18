# Project Legacy Launcher 4.3.2

- Added Tauri 2 signed auto-update support.
- Checks the configured GitHub Releases `latest.json` after launcher startup.
- Shows available version and release notes in the existing update bar.
- Downloads and installs updates only after the user confirms.
- Verifies update signatures through the official Tauri updater plugin.
- GitHub Actions now creates signed Windows updater artifacts and uploads `latest.json` when a `v4.3.2` (or later) tag is pushed.


## Release status

These are source-code changes for the 4.3.2 release candidate. The public updater key and both private-key GitHub Actions secrets are configured. Windows CI run #14 passed Rust tests, `cargo check`, and signed NSIS packaging; the Rust and Tauri CLI caches were restored successfully. The signed test installer is available as a GitHub Actions artifact.

The updater is not yet live because no Tauri `v4.3.2` release or `latest.json` feed has been published. After the Build 4.5 archive/hash and CSP runtime check are complete, publish the signed `v4.3.2` release, then publish a newer signed Tauri version (for example `v4.3.3`) to perform the real update test. This branch is still a draft release candidate, not a production release.
