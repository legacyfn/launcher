@echo off
setlocal
cd /d "%~dp0"

echo === Project Legacy Launcher - Tauri Windows build ===
echo.

where cargo >nul 2>nul
if errorlevel 1 (
  echo Rust is not installed.
  echo Install it from https://rustup.rs then re-run this script.
  echo ^(Pick the default "MSVC" toolchain when asked.^)
  pause
  exit /b 1
)

where cargo-tauri >nul 2>nul
if errorlevel 1 (
  echo Installing the Tauri CLI ^(first run only, this can take a few minutes^)...
  cargo install tauri-cli --version "^2" --locked
  if errorlevel 1 (pause & exit /b 1)
)

echo Running Rust tests...
cargo test --manifest-path src-tauri\Cargo.toml
if errorlevel 1 (
  echo Tests failed. The release build has been stopped.
  pause
  exit /b 1
)

echo Building release installer...
pushd src-tauri
if defined TAURI_SIGNING_PRIVATE_KEY (
  echo Updater signing key detected - generating signed update artifacts.
  cargo tauri build
) else (
  echo No updater signing key set - building a normal test installer.
  echo This installer will NOT be able to self-update until signed with the configured key.
  cargo tauri build --config tauri.local.conf.json
)
set BUILD_RESULT=%ERRORLEVEL%
popd
if not "%BUILD_RESULT%"=="0" (pause & exit /b %BUILD_RESULT%)

echo.
echo Done. Installer is in src-tauri\target\release\bundle\nsis\
explorer "src-tauri\target\release\bundle\nsis"
pause
