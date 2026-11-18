# Project Legacy Launcher — release readiness

## Implemented in this migration pass

- Tauri v2 backend and Electron-compatible `window.launcher` bridge.
- CamelCase settings and endpoint serialization compatibility.
- Windows DPAPI password encryption for newly saved credentials, with migration attempts for legacy Electron Base64-only and safeStorage/DPAPI values under the same Windows user profile.
- Console access-code verification with AES-256-GCM/scrypt validation and failed-attempt throttling; its key is stored with DPAPI.
- Original `Injector.exe`, `projectlegacy.dll` and `edit.dll.enc` copied exactly and verified against SHA-256 hashes.
- Original injector invocation restored for automatic delayed Redirector loading and manual Console loading after DPAPI-protected access-code verification; temporary decrypted Console DLLs are removed with retry.
- Folder selection, build-root detection, install marker, resumable downloader, pause/cancel handling, SHA-256 download gate and 7z extraction path validation.
- Game child-process monitoring, game stop, compatibility-mode processor affinity.
- Health/news/clients/Discord/profile polling and reconnecting match-event SSE stream.
- Cached news and cached live state replay so UI listeners do not miss early events.
- Diagnostics/log retrieval, original WebView backgrounds/music, single-instance window focus.
- Tauri capability file and Windows CI tests/build.

## Explicitly not active

- Anti-cheat placeholder processes or suspension. The optional legacy `useStubs` setting is accepted for config compatibility but does not spawn or suspend game/security processes.
- Windows Defender exclusions.
- The updater is integrated in source and its generated public key is now configured; it cannot check a valid feed until a signed GitHub release is published.
- `anticrash.dll` (not present in the supplied Electron ZIP).

The original injector and console binaries remain bundled and integrity checked. The Redirector and Console use the original `Injector.exe -p <pid> -i <dll-path>` invocation. The Defender exclusion command remains a no-op, and `anticrash.dll` is absent from the supplied original ZIP.

## Release blockers / owner actions

1. Publish a genuine `4.5.7z` archive and set its exact 64-character SHA-256 in `src-tauri/resources/endpoints.json`. Downloads are disabled until this is done.
2. Run `cargo test --manifest-path src-tauri/Cargo.toml` and `cargo tauri build` on Windows. This environment does not have Rust installed, so no Rust build is claimed here.
3. Install and smoke-test the NSIS package on a clean Windows 10/11 machine: first run, config migration, folder chooser, download/pause/resume/cancel, extraction, launch/stop, notification preferences and offline news.
4. Complete the updater-key and GitHub secret setup described in the Version 4.3.2 section below, then publish the signed `v4.3.2` release.
5. Review the game-auth command-line argument behavior: the legacy password argument can be visible to local process-inspection tools even though the stored config is DPAPI-protected.

## Follow-up compile-error patch (2026-11-18)

Applied fixes for the four Rust diagnostics reported by the Windows build:

- Removed the unused `std::sync::atomic::Ordering` import.
- Compared the installation-phase mutex guard through `phase.as_str()`.
- Changed `inject_module` to obtain managed state from its owned `AppHandle`, so the async Tauri command no longer has a borrowed `State` input while returning a plain JSON value.
- Added the required 32-byte output length to `scrypt::Params::new(15, 8, 1, 32)`, matching the 32-byte derived key buffer.

These edits address the diagnostics in the supplied build log. A Rust compiler is not installed in this working environment, so the patched source has not been compiled here; rerun `build.bat` or the Windows CI workflow to identify any subsequent diagnostics.

## Static checks performed

- JSON parsing for `tauri.conf.json`, `endpoints.json` and capability manifest.
- YAML parsing for GitHub Actions workflow.
- `node --check` for the Tauri bridge.
- Rust command registration and frontend API name cross-check.
- SHA-256 comparison of the three bundled binaries against the source Electron ZIP.

A successful Windows CI build remains mandatory before release.


## Latest changes

- Added a mandatory persisted first-run account-details gate before the launcher dashboard (tracked by `setupComplete`, including when importing an older Electron configuration). It securely saves the supplied game-account password with Windows DPAPI, then presents existing-build selection or Build 4.5 download. Setup is marked complete only after the backend confirms that the selected folder contains a complete build.
- The current server configuration has no launcher-authentication endpoint, so the first-run form stores game-account credentials but does not validate a remote launcher identity. Do not describe this as server-side authentication until an auth API is connected.
- Folder checking, file verification, and diagnostics now run filesystem work on blocking workers; scans are bounded and avoid symlink traversal. The UI displays command errors rather than swallowing them.
- Rust/Windows compilation and interactive installer testing remain required on a Windows machine with the Rust toolchain.


## UX follow-up changes (2026-11-18)

- Fixed repeated first-run onboarding by making its completion decision depend on persisted account/build/notification state and by calling `complete_setup` to recover valid older installations.
- Added a persisted launcher display username and match-notification opt-in/out flow.
- Removed user-editable game launch arguments; the backend always uses `DEFAULT_ARGS`.
- Removed the expanded-navigation toggle and hid diagnostics, log-copy and verification controls from the normal user interface while retaining the in-game Console unlock.
- Rust compilation and end-to-end Windows installer tests still need to be run on a Windows host with the Rust toolchain.


## User-facing settings and first-run update

- The first-run flow now requires a launcher display name, game-account username/password, a saved match-notification choice, and a valid installed build. It is only reopened when one of those prerequisites is genuinely missing.
- Corrected the frontend setup flag name to `setupComplete` (the Rust `PublicConfig` camelCase field), preventing the completed onboarding state from being misread on later starts.
- Launch parameters are hard-coded in the launcher and legacy custom arguments are reset on load. The compatibility-mode setting is not user-configurable.
- Removed the obsolete Help/diagnostics/reset/compatibility/Defender handlers and styles from the UI. The in-game Console unlock remains available as a normal game feature.
- Removed the unused navigation compact-mode CSS and forced-mode code; the normal full navigation is fixed.

## Version 4.3.2 — Tauri auto-updater integration

- `tauri-plugin-updater` v2 requires Rust 1.90 or newer; `Cargo.toml` now declares `rust-version = "1.90"`.
- The generated updater public key is committed in `src-tauri/tauri.conf.json` and must remain unchanged for future updates.

**Implemented in source:** `tauri-plugin-updater` v2 is registered, signed updater artifacts are enabled for release builds, and Rust commands check the configured GitHub `latest.json`, download and install updates, and emit progress events. The frontend checks on startup and lets the user choose when to install. The GitHub Actions workflow tests the project and publishes the Windows NSIS installer plus `latest.json` on a `v*` tag.

**Still requires repository-owner setup before publication:**

1. The generated Tauri key pair's public key is now configured in `src-tauri/tauri.conf.json`.
2. Add `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` as GitHub Actions secrets in `legacyfn/launcher`. Never commit the private key.
3. Back up the private key file and its password securely.
4. Run the Windows CI/build and installer smoke tests.
5. Only after the checks pass and secrets are configured, publish tag `v4.3.2`.
6. Ensure the GitHub Release and `latest.json` are publicly readable. A private repository cannot serve a public update feed to unauthenticated launcher clients.

The private signing key is deliberately not included in this package. The version bump and source integration are prepared, but an actual signed release has not been published from this environment.
