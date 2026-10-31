# Builds the app and the NSIS installer, then copies it to Rewind-Setup.exe.
# Usage: powershell -File scripts/package.ps1
$ErrorActionPreference = 'Stop'
Set-Location (Join-Path $PSScriptRoot '..')
cargo tauri build
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
$dir = 'src-tauri/target/release/bundle/nsis'
$built = Get-ChildItem $dir -Filter 'Rewind_*-setup.exe' | Sort-Object LastWriteTime -Descending | Select-Object -First 1
Copy-Item $built.FullName (Join-Path $dir 'Rewind-Setup.exe') -Force
Write-Host "Installer: $dir/Rewind-Setup.exe"
