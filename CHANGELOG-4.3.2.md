# Project Legacy Launcher 4.3.2

- Added Tauri 2 signed auto-update support.
- Checks the configured GitHub Releases `latest.json` after launcher startup.
- Shows available version and release notes in the existing update bar.
- Downloads and installs updates only after the user confirms.
- Verifies update signatures through the official Tauri updater plugin.
- GitHub Actions now creates signed Windows updater artifacts and uploads `latest.json` when a `v4.3.2` (or later) tag is pushed.


## Release status

These are source-code changes for the 4.3.2 release candidate. The generated public key is now configured in `src-tauri/tauri.conf.json`. The updater becomes operational after the owner configures the two private-key GitHub Actions secrets, the Windows CI/build passes, and the signed `v4.3.2` release is published. This branch is not itself a published or signed updater release.
