$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

cargo build --workspace --release

$dist = Join-Path $root "dist"
New-Item -ItemType Directory -Force -Path $dist | Out-Null

Copy-Item "target\release\ec-su_axb35-server.exe" (Join-Path $dist "ec-su_axb35-server.exe") -Force
Copy-Item "target\release\ec-su_axb35-server.exe" (Join-Path $dist "evox2-control.exe") -Force
Copy-Item "target\release\evox2ctl.exe" (Join-Path $dist "evox2ctl.exe") -Force
Copy-Item "target\release\ec-su_axb35-win-client.exe" (Join-Path $dist "ec-su_axb35-win-client.exe") -Force

Write-Host "Release binaries copied to $dist"
Write-Host "PawnIO is an external official dependency and is not bundled."
Write-Host "If makensis is available, compile installer.nsi next."
