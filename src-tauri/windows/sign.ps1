# Code-signing hook, called by the Tauri bundler for every binary it produces
# (the app exe and Rewind-Setup.exe). Without a certificate configured it is a
# no-op, so unsigned builds keep working.
#
# To sign, set either:
#   REWIND_SIGN_PFX       path to a .pfx file  (+ REWIND_SIGN_PFX_PASSWORD)
#   REWIND_SIGN_THUMBPRINT thumbprint of a cert in the user/machine store
# Optional: REWIND_SIGN_TIMESTAMP (default http://timestamp.digicert.com)
param([Parameter(Mandatory = $true)][string]$File)

$ErrorActionPreference = 'Stop'
$ts = if ($env:REWIND_SIGN_TIMESTAMP) { $env:REWIND_SIGN_TIMESTAMP } else { 'http://timestamp.digicert.com' }

if (-not $env:REWIND_SIGN_PFX -and -not $env:REWIND_SIGN_THUMBPRINT) {
    Write-Host "sign.ps1: no certificate configured, leaving $File unsigned"
    exit 0
}

$signtool = Get-ChildItem "${env:ProgramFiles(x86)}\Windows Kits\10\bin\*\x64\signtool.exe" -ErrorAction SilentlyContinue |
    Sort-Object FullName -Descending | Select-Object -First 1
if (-not $signtool) { throw 'signtool.exe not found (install the Windows SDK)' }

$args = @('sign', '/fd', 'SHA256', '/tr', $ts, '/td', 'SHA256')
if ($env:REWIND_SIGN_PFX) {
    $args += @('/f', $env:REWIND_SIGN_PFX)
    if ($env:REWIND_SIGN_PFX_PASSWORD) { $args += @('/p', $env:REWIND_SIGN_PFX_PASSWORD) }
} else {
    $args += @('/sha1', $env:REWIND_SIGN_THUMBPRINT)
}
& $signtool.FullName @args $File
if ($LASTEXITCODE -ne 0) { throw "signtool failed with $LASTEXITCODE" }
