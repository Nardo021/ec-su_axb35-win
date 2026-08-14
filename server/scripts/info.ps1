#!/usr/bin/env pwsh
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$ctl = Join-Path $root "target\release\evox2ctl.exe"
if (-not (Test-Path $ctl)) {
    $ctl = "evox2ctl"
}
& $ctl diagnose
& $ctl status
