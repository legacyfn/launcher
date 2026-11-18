$ErrorActionPreference = 'Stop'

$tauriDir = Join-Path $PSScriptRoot '..\src-tauri'
$privateKeyPath = Join-Path $env:USERPROFILE '.tauri\projectlegacy-updater.key'

New-Item -ItemType Directory -Force -Path (Split-Path $privateKeyPath) | Out-Null

if (Test-Path $privateKeyPath) {
    throw "An updater private key already exists at '$privateKeyPath'. Back it up before generating another key. Do not overwrite it: existing launcher installs trust its matching public key."
}

Push-Location $tauriDir
try {
    cargo tauri signer generate -w $privateKeyPath
    if ($LASTEXITCODE -ne 0) {
        throw 'Tauri updater key generation failed.'
    }
}
finally {
    Pop-Location
}

Write-Host ''
Write-Host 'Updater key pair generated.' -ForegroundColor Green
Write-Host "Keep this private key safe: $privateKeyPath"
Write-Host 'Copy the PUBLIC key printed above into src-tauri/tauri.conf.json -> plugins.updater.pubkey.'
Write-Host 'Never commit or send the private key. Add its contents as GitHub Actions secret TAURI_SIGNING_PRIVATE_KEY.'
Write-Host 'Add the chosen key password as GitHub Actions secret TAURI_SIGNING_PRIVATE_KEY_PASSWORD.'
