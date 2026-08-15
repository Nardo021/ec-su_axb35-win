param(
    [Parameter(Mandatory = $true)]
    [string[]]$Path,
    [string]$Pfx = $env:SIGNING_PFX,
    [string]$Password = $env:SIGNING_PASSWORD,
    [string]$TimestampUrl = "http://timestamp.digicert.com"
)

$ErrorActionPreference = "Stop"

if ([string]::IsNullOrWhiteSpace($Pfx)) {
    Write-Host "No SIGNING_PFX present; skipping Authenticode signature."
    exit 0
}

$pfxPath = $Pfx
$cleanup = $false
if (-not (Test-Path -LiteralPath $Pfx)) {
    $pfxPath = Join-Path $env:TEMP "evox2-signing.pfx"
    [IO.File]::WriteAllBytes($pfxPath, [Convert]::FromBase64String($Pfx))
    $cleanup = $true
}

$signtool = @(
    "${env:ProgramFiles(x86)}\Windows Kits\10\bin\x64\signtool.exe",
    "${env:ProgramFiles(x86)}\Windows Kits\10\App Certification Kit\signtool.exe"
) | Where-Object { Test-Path $_ } | Select-Object -First 1

if (-not $signtool) {
    $signtool = Get-Command signtool.exe -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Source
}

if (-not $signtool) {
    throw "signtool.exe was not found. Install the Windows SDK signing tools."
}

try {
    foreach ($file in $Path) {
        if (-not (Test-Path -LiteralPath $file)) {
            throw "File not found: $file"
        }
        $signArgs = @(
            "sign",
            "/fd", "SHA256",
            "/td", "SHA256",
            "/tr", $TimestampUrl,
            "/f", $pfxPath
        )
        if (-not [string]::IsNullOrWhiteSpace($Password)) {
            $signArgs += @("/p", $Password)
        }
        $signArgs += $file
        & $signtool @signArgs
        if ($LASTEXITCODE -ne 0) {
            throw "signtool failed for $file with exit code $LASTEXITCODE"
        }
        Write-Host "Signed $file"
    }
}
finally {
    if ($cleanup -and (Test-Path -LiteralPath $pfxPath)) {
        Remove-Item -LiteralPath $pfxPath -Force
    }
}
