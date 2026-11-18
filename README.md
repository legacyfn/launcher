# Project Legacy Launcher — Tauri 2 migration

This project migrates the original Project Legacy Electron launcher to a single Tauri 2 desktop application. The existing `ui/index.html` design and frontend behavior have been retained where possible; `ui/api.js` replaces the Electron preload bridge with Tauri commands and events. The native backend is Rust.

## Project layout

```text
ui/                         Existing launcher UI, styles, scripts and web assets
  api.js                    Electron-preload compatibility layer for Tauri
  assets/                   Logo, backgrounds, music and other WebView media
src-tauri/
  src/                      Tauri commands and Rust services
  resources/
    endpoints.json          Server endpoints and build archive integrity hash
    assets/                 Rust-side copies of backgrounds/live media
    bin/                    Original supplied injector/module files (bundled, hash checked)
  capabilities/default.json Minimal Tauri v2 WebView permissions
build.bat                   Windows test + release installer build
.github/workflows/build.yml Windows CI build and release workflow
```

## Migrated launcher features

| Electron behavior | Tauri implementation |
|---|---|
| Existing dashboard, settings, translations, news and shop UI | Preserved from the supplied Electron frontend |
| Preload `launcher.*` API | `ui/api.js` maps API calls to Tauri `invoke` and `listen` |
| Configuration file | Same `launcher-config.json`, camelCase JSON compatibility, import from the original Electron user-data folders, and support-file cache migration |
| Password at rest | Windows DPAPI CurrentUser storage for new credentials; migration reads the legacy Electron safeStorage DPAPI blob under the same Windows user account when compatible, otherwise prompts for one-time re-entry |
| Folder selection and validation | Tauri dialog + Rust build-root detection |
| Download, resume, pause and cancel | Async Rust downloader using HTTP Range and retry logic |
| Archive integrity | SHA-256 gate before extraction |
| Archive extraction | 7z-only extractor with path validation and incomplete-install marker |
| Game launch and stop | Direct process launch, child-process monitoring and process-tree stop |
| Compatibility mode | Not exposed to end users; launcher-managed defaults only |
| Health, clients, Discord and profile | Rust HTTPS polling |
| Match/server events | Reconnecting SSE stream with a latest-state cache and UI replay |
| News | HTTPS polling with a local cache for offline startup |
| Diagnostics and log copy | Internal pre-launch integrity validation only; no user-facing debug/log panel |
| Background slideshow and music | Original assets copied into the WebView distribution |
| Single application instance | Tauri single-instance plugin; second start focuses the main window |

## Original injector and module assets

The exact files supplied in the Electron archive are present under `src-tauri/resources/bin/`:

- `Injector.exe`
- `projectlegacy.dll`
- `edit.dll.enc`

Their SHA-256 hashes are checked by the diagnostics/preflight code. They are bundled as original project resources and are not stripped from the Tauri installer. The Tauri backend restores the original `Injector.exe -p <pid> -i <dll-path>` invocation for delayed Redirector loading and manual Console loading after the access code is verified. The original archive did not contain `anticrash.dll`; the UI therefore hides that optional tool. The launcher does not spawn or suspend anti-cheat placeholder processes and does not create or change Windows Defender exclusions.

The legacy `useStubs` setting is retained in the configuration for migration compatibility, but Tauri does not launch or suspend placeholder anti-cheat processes. The UI marks that option as legacy and disables the checkbox.

## Required server configuration before download is enabled

Edit `src-tauri/resources/endpoints.json` after publishing the build archive:

```json
{
  "buildArchiveUrl": "https://your-host.example/builds/4.5.7z",
  "buildArchiveSha256": "PUT_THE_REAL_64_CHARACTER_SHA256_HERE"
}
```

Keep the other existing endpoint properties. The archive must be a genuine 7z archive; changing a `.rar` filename to `.7z` does not convert it. The launcher checks both the 7z signature and SHA-256. Downloads intentionally stay disabled while the SHA-256 is blank or malformed. Do not publish a guessed hash.

## Windows build validation

GitHub Actions run #14 succeeded on 2026-11-18 after the CSP and password-memory changes: Rust tests, `cargo check`, and signed Windows NSIS packaging all passed. Both the Rust build cache and Tauri CLI cache were restored successfully; the CLI installation was skipped on a cache hit. The repository owner has confirmed clean Windows installation, Electron configuration migration, and existing game-folder tests. A runtime CSP regression check on the latest signed artifact is still required.

New and migrated Windows credentials are stored with DPAPI CurrentUser. Plaintext strings held by the launcher are zeroized after use where possible. The legacy game client still requires `-AUTH_PASSWORD` as a process command-line argument, so sufficiently privileged local process-inspection tools may be able to read it while the game runs. A different supported client/server authentication interface is needed to remove that limitation.

## Windows build

Requirements: Windows 10/11, Rust 1.90 or newer with the MSVC toolchain, and the Microsoft C++ build tools / WebView2 runtime expected by Tauri. Rust 1.90+ is required by the Tauri updater plugin.

Run:

```bat
build.bat
```

The script checks for Rust, installs the Tauri CLI when missing, runs `cargo test`, then changes into `src-tauri` before invoking `cargo tauri build` (the Tauri config is located in that directory). It produces the NSIS installer under:

```text
src-tauri/target/release/bundle/nsis/
```

GitHub Actions runs Rust tests and a Windows `cargo check` for pull requests and tags. A tag beginning with `v` additionally builds the Windows NSIS installer, creates the signed updater artifact, and publishes a GitHub Release with `latest.json` after both signing secrets are configured.

## Production items that still need owner action

1. **Build archive:** finish the genuine RAR-to-7z conversion, publish the `.7z` file, and fill in its actual SHA-256 in `endpoints.json`.
2. **CSP validation:** build the new Tauri CSP and check it in the installed app; verify fonts, backgrounds, audio/video, controls, and Tauri IPC.
3. **Credential decision:** DPAPI protects saved credentials and launcher-owned plaintext copies are zeroized after use. The legacy game client still receives `-AUTH_PASSWORD` on its process command line; accept/document that residual risk or provide a supported alternative authentication flow.
4. **Self-updater:** signing secrets are configured and have successfully produced a signed installer. A real update cycle still requires the Tauri 4.3.2 release and a newer signed Tauri release to update to.
5. **Final publication:** only after the archive hash, CSP regression test, credential-risk decision, and CI pass should PR #1 be marked ready, merged, and tagged for release.

## What was not claimed

This is a release candidate, not yet a published production release. Windows CI has passed and a signed NSIS test installer has been built. The repository owner reports that clean installation, Electron configuration migration, and existing-game-folder testing are complete. The new CSP still needs a fresh runtime check, and the updater has not yet completed a real version-to-version installation.


## First-run setup and folder-check recovery

On a fresh installation, the launcher now opens a persisted first-run setup gate before the dashboard (including when importing an older Electron configuration). The user must enter the game-account username/email and password, then either select an existing Build 4.5 folder or select a destination and download the configured Build 4.5 archive. The password is stored through the existing Windows DPAPI path. The first-run gate asks for the username/email and password even when an older Electron configuration was imported, then keeps the setup complete flag so the wizard does not repeat on every launch.

**Authentication limitation:** this project currently exposes a public profile lookup but no server-side login/authentication endpoint. The first-run form therefore requires and securely stores the game-account credentials, but it does not claim to validate a Project Legacy launcher identity remotely. A real launcher-account sign-in requires a documented authentication API and server-side session/token validation.

Folder verification, file verification, and diagnostics now execute filesystem scans on blocking workers. Recursive scans are bounded and avoid following symlinks/reparse points. UI handlers show the returned error instead of silently swallowing it.


## User-facing configuration policy

- Game launch arguments are hard-coded in the Rust launcher (`args::DEFAULT_ARGS`); users cannot edit them in the UI. Existing custom values in migrated configs are reset to the launcher default.
- The extended-navigation toggle and diagnostics/log-copy controls are not exposed in the normal UI. The actual in-game Console unlock remains available because it is a launcher feature, not a diagnostic panel.
- First-run setup asks for a launcher display username, game-account username/email and password, then asks whether to enable match-open notifications, then lets the user select or download the configured build.
- The match-notification preference and its “asked” state are stored in the launcher config. A completed installation skips onboarding on subsequent launches.
- Existing installations with saved credentials and builds receive the notification question once; they are not forced through the account/build setup again.


## Fixed end-user configuration

End users cannot edit launch arguments or toggle compatibility mode. The launcher applies the centrally defined argument list. The non-functional expanded-navigation option and obsolete Help/debug/diagnostics controls have been removed; the normal full navigation is always used. The in-game Console unlock remains available.

The first-run wizard asks for a launcher display name, game-account username/password, the user's match-notification preference, and then an existing or downloadable game build. The completion flag is read using the correct `setupComplete` JSON property and is persisted only after all required setup steps are complete.

## Tauri automatic launcher updates — version 4.3.2

The Tauri 2 updater is now integrated in the Rust backend. On startup the launcher checks the GitHub Releases manifest. If a newer signed version exists, the existing update bar shows the version and release notes. The user explicitly chooses **Download & Install**; the launcher downloads the Windows updater artifact, verifies its Tauri signature, and starts the NSIS update. The game build download/install flow is separate and is not changed by launcher self-updates.

### Release repository and endpoint

The endpoint is configured in `src-tauri/tauri.conf.json`:

```text
https://github.com/legacyfn/launcher/releases/latest/download/latest.json
```

This repository path is taken from the supplied original Electron `package.json` (`owner: legacyfn`, `repo: launcher`). GitHub Releases and its update assets must be publicly readable by all launcher clients. If the GitHub repository is private, do not embed a personal access token in the launcher; instead host `latest.json` and the signed updater asset on a public HTTPS endpoint.

### One-time signing-key setup

The updater public key is configured in `src-tauri/tauri.conf.json`. The private-key GitHub Actions secrets have been configured and verified by a successful signed Windows installer build. Keep a secure backup of the private key file and password. Never commit or share the private key, and never replace the public key after users have installed a release signed with it.

### Publishing version 4.3.2

The workflow `.github/workflows/build.yml` first runs Rust tests and `cargo check`. After the GitHub Actions signing secrets are configured, a push of tag `v4.3.2` runs `tauri-apps/tauri-action@v1`, builds the signed Windows NSIS installer and updater artifact, creates the GitHub Release, and uploads `latest.json` with the signed platform metadata. The release tag, `Cargo.toml` version and `tauri.conf.json` version are checked for an exact match.

After the project changes and signing secrets are in GitHub:

```powershell
git tag v4.3.2
git push origin v4.3.2
```

The release workflow publishes the update. The public key and signing secrets are already configured. Do not push the tag until the genuine Build 4.5 archive hash, CSP regression test, credential-risk decision, and fresh CI checks are complete.

### Local builds

`build.bat` creates a regular unsigned test installer if `TAURI_SIGNING_PRIVATE_KEY` is not set. That local build is useful for validating the UI and game launcher, but it is not capable of publishing self-updates. When the signing-key environment variable is present, `build.bat` creates the signed updater artifacts too. The GitHub release workflow is the recommended publishing path.

### Runtime behavior and limitations

- Update check runs in the background after the launcher UI appears.
- No update is installed automatically; the player confirms installation.
- Download progress and installation status are shown in the existing update bar.
- Update artifacts are signature-verified by the Tauri updater before installation.
- On Windows, the NSIS updater closes the launcher as part of installation. The game client is a separate process.
- Version 4.3.2 cannot update old Electron installs directly. Users need to install the first Tauri build once; later Tauri releases can self-update.
- The updater endpoint returns no updates until a public GitHub Release with `latest.json` is published and the signing public key matches the release signing key.
