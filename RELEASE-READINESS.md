# Project Legacy Launcher — release readiness

## Version 4.3.5 release preparation

The `release/v4.3.5` branch is prepared from the current `main` branch. This is a release candidate preparation only; no tag or public release has been created.

### Included changes carried into this release

- Fixed native Windows match-open notifications and made the notification preference save/restore reliably.
- Pauses launcher animations, slideshow timers, particle rendering and background video while the launcher window is inactive; resumes them when the launcher returns to focus.
- Updated the launcher frontend styling to match the Project Legacy web design.
- Added dedicated Solo, Duos and Squad artwork under `ui/assets/gamemodes/`.
- Fixed the Tauri async `open_link` command to return `Result<bool, String>`, resolving the async command compile error.
- Kept the updater signing public key, HTTPS release feed and Build 4.5 archive SHA-256 configuration unchanged.

### v4.3.5 version alignment

- `src-tauri/Cargo.toml`: package version must be `4.3.5`.
- `src-tauri/tauri.conf.json`: Tauri app version must be `4.3.5`.
- `.github/workflows/build.yml`: signed test artifact name is `tauri-4.3.5-signed-windows-test`.
- The release workflow validates the pushed `v4.3.5` tag against both version fields and verifies the published `latest.json`, installer and signature.

### Checks still required before publishing

1. Run the Windows GitHub Actions workflow on this branch; require Rust tests, `cargo check` and signed NSIS packaging to pass.
2. Install the resulting signed artifact on Windows and smoke-test first-run setup, existing-install migration, game-folder selection, launch/stop, downloads, update UI and Tauri IPC.
3. Verify Windows notification permission, enabling/disabling the match-notification toggle, and a real match-open toast.
4. Verify the Solo, Duos and Squad images load from the packaged WebView assets.
5. Confirm animations/video pause when the launcher loses focus and resume when it becomes active.
6. Verify the signed updater release contains `latest.json`, the Windows installer and `.exe.sig`; perform a real update test only after the release is published.

Do not publish the tag until the Windows CI and manual smoke tests pass. The updater private signing key must remain in GitHub Actions secrets and must never be committed.

---

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
2. Windows CI run #14 passed on 2026-11-18 after the CSP and password-memory changes: Rust tests, cargo check, signed NSIS packaging, and artifact upload all succeeded. The Rust build cache and Tauri CLI cache were both restored successfully. The remaining CSP check is a runtime UI smoke test on the latest signed artifact.
3. The owner reports that clean Windows installation, Electron configuration migration, and an existing game installation have been tested. Repeat the UI smoke test on the new CSP build, including fonts, backgrounds, audio/video, controls, and Tauri IPC.
4. The updater public key and GitHub signing secrets are configured and were validated by a successful signed build. Publish the Tauri release only after the archive, CSP runtime check, and credential-risk decision are complete; then test an update to a newer signed version.
5. Review and accept the remaining game-auth risk: DPAPI protects the saved config and launcher-owned plaintext copies are zeroized after use, but the legacy `-AUTH_PASSWORD` process argument can still be visible to sufficiently privileged local process-inspection tools.

## Follow-up compile-error patch (2026-11-18)

Applied fixes for the four Rust diagnostics reported by the Windows build:

- Removed the unused `std::sync::atomic::Ordering` import.
- Compared the installation-phase mutex guard through `phase.as_str()`.
- Changed `inject_module` to obtain managed state from its owned `AppHandle`, so the async Tauri command no longer has a borrowed `State` input while returning a plain JSON value.
- Added the required 32-byte output length to `scrypt::Params::new(15, 8, 1, 32)`, matching the 32-byte derived key buffer.

These edits addressed the diagnostics in the supplied build log. They were subsequently compiled in Windows CI run #13, where `cargo test`, `cargo check`, and signed NSIS packaging all passed.

## Static checks performed

- JSON parsing for `tauri.conf.json`, `endpoints.json` and capability manifest.
- YAML parsing for GitHub Actions workflow.
- `node --check` for the Tauri bridge.
- Rust command registration and frontend API name cross-check.
- SHA-256 comparison of the three bundled binaries against the source Electron ZIP.

Windows CI run #14 succeeded on 2026-11-18 after the security hardening changes: Rust tests, cargo check, signed NSIS packaging, and artifact upload passed. The Rust build cache and Tauri CLI cache both produced cache hits.


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

**Current release status:**

1. The updater public key is configured in `src-tauri/tauri.conf.json`.
2. The signing secrets have been configured and were verified by a successful signed Windows installer build in GitHub Actions.
3. Keep a secure backup of the private key and password; do not commit or share them.
4. A Tauri `latest.json` feed is not published yet. The public latest release remains Electron 4.3.1.
5. A real updater install test requires the published Tauri 4.3.2 and then a newer signed Tauri release (for example 4.3.3).

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
2. Install the latest signed test artifact and confirm the CSP does not block fonts, backgrounds, audio/video, UI controls, notifications, or Tauri IPC. Windows CI run #14 already passed after these security changes.
4. Review and accept the documented legacy game command-line password exposure, or provide a supported client/server alternative authentication interface.
5. Publish Tauri 4.3.2 only after the above checks and archive hash are complete. Then test a real update to a newer signed Tauri version.
6. Keep PR #1 in Draft until the remaining checks and product-owner decisions are complete.
