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

Windows CI run #11 succeeded on 2026-11-18: Rust tests, cargo check, and signed NSIS packaging passed. A fresh Windows CI run is required after the security hardening changes in this commit.


## Latest changes

- Added a mandatory persisted first-run account-details gate before the launcher dashboard (tracked by `setupComplete`, including when importing an older Electron configuration). It securely saves the supplied game-account password with Windows DPAPI, then presents existing-build selection or Build 4.5 download. Setup is marked complete only after the backend confirms that the selected folder contains a complete build.
- The current server configuration has no launcher-authentication endpoint, so the first-run form stores game-account credentials but does not validate a remote launcher identity. Do not describe this as server-side authentication until an auth API is connected.
- Folder checking, file verification, and diagnostics now run filesystem work on blocking workers; scans are bounded and avoid symlink traversal. The UI displays command errors rather than swallowing them.
- Windows compilation and signed NSIS packaging passed in GitHub Actions. The repository owner reports that clean installation, Electron configuration migration, and existing game-folder tests are complete. Re-test the interface against the new CSP.


## UX follow-up changes (2026-11-18)

- Fixed repeated first-run onboarding by making its completion decision depend on persisted account/build/notification state and by calling `complete_setup` to recover valid older installations.
- Added a persisted launcher display username and match-notification opt-in/out flow.
- Removed user-editable game launch arguments; the backend always uses `DEFAULT_ARGS`.
- Removed the expanded-navigation toggle and hid diagnostics, log-copy and verification controls from the normal user interface while retaining the in-game Console unlock.
- Windows compilation and signed NSIS packaging passed in GitHub Actions. The repository owner reports that clean installation, Electron configuration migration, and existing game-folder tests are complete. Re-test the interface against the new CSP.


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
4. Re-run Windows CI after the CSP and password-memory changes, then repeat the UI smoke test with that installer. The initial clean-install, Electron migration, and existing-game-folder tests are owner-confirmed as complete.
5. Only after the checks pass and secrets are configured, publish tag `v4.3.2`.
6. Ensure the GitHub Release and `latest.json` are publicly readable. A private repository cannot serve a public update feed to unauthenticated launcher clients.

The private signing key is deliberately not included in this package. The version bump and source integration are prepared, but an actual signed release has not been published from this environment.


## Security, updater, and final review — 2026-11-18

### Content Security Policy
- Added a Tauri-enforced CSP in `tauri.conf.json`: local scripts only, Google Fonts CSS/font hosts only, local/data/blob images, local media, Tauri IPC for WebView connections, and no plugins/objects or external form actions.
- The application has one inline application script and one inline style block; no inline event-handler attributes were found. Tauri adds hashes/nonces to bundled scripts/styles at build time.
- Dynamic style attributes remain allowed because the existing UI sets slideshow and layout styles at runtime.
- The CSP still needs a runtime regression test on the new installer: fonts, backgrounds, audio/video, buttons, and Tauri IPC must all work.

### Password and login handling review
- Newly saved and migrated Windows credentials use DPAPI CurrentUser. The frontend receives only password-presence/re-entry flags, not the decrypted password.
- Plaintext password strings and launcher-built argument vectors are now held in zeroizing wrappers to reduce the lifetime of launcher-owned copies.
- **Residual risk:** the legacy game client still requires `-AUTH_PASSWORD=...` as a process command-line argument. Local process-inspection tools running with sufficient rights may be able to read it while the game is running. Zeroizing launcher memory does not remove that operating-system command-line exposure.
- The current server configuration has no launcher-authentication endpoint. Saving credentials is therefore not server-side launcher login validation.
- Removing the command-line exposure requires a supported alternative authentication interface in the game client/server. The legacy auth arguments must not be changed blindly because that could break game login. This limitation remains a documented release decision.

### Updater review and validation
- GitHub Actions successfully built a signed NSIS installer and generated its signature with the configured private-key secrets.
- The configured updater feed is the public GitHub Releases `latest.json` endpoint. The latest published release is still Electron 4.3.1 and contains `latest.yml`, not a Tauri `latest.json` feed.
- Added a post-release CI job that checks for `latest.json`, the Windows installer, its signature, a matching release/manifest version, and an HTTPS `windows-x86_64` updater entry after a version tag is published.
- **A real updater installation is not yet tested.** It requires Tauri 4.3.2 to be published and a newer signed Tauri version (for example 4.3.3) to be published afterwards, or a separate test feed. No public release/tag has been created in this pass.

### Owner-confirmed Windows tests
- Clean Windows installation: completed.
- Existing Electron configuration migration: completed.
- Existing game installation selection/use: completed.
- These are manual tests confirmed by the repository owner on 2026-11-18, separate from automated CI.

### Remaining release gates
1. Publish the genuine Build 4.5 archive after the RAR-to-7z conversion and set its actual 64-character SHA-256 in `src-tauri/resources/endpoints.json`.
2. Run the new Windows CI build after this security commit.
3. Install that new build and confirm there are no CSP-blocked fonts, backgrounds, audio/video, UI controls, or Tauri IPC calls.
4. Review and accept the documented legacy game command-line password exposure, or provide a supported client/server alternative authentication interface.
5. Publish Tauri 4.3.2 only after the above checks and archive hash are complete. Then test a real update to a newer signed Tauri version.
6. Keep PR #1 in Draft until the remaining checks and product-owner decisions are complete.
