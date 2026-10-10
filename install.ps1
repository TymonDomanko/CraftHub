# Install Craft Hub for the current user on Windows (no admin needed). In PowerShell:
#   irm https://raw.githubusercontent.com/TymonDomanko/CraftHub/main/install.ps1 | iex
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'

$repo = 'TymonDomanko/CraftHub'
$isArm = ($env:PROCESSOR_ARCHITECTURE -eq 'ARM64') -or ($env:PROCESSOR_ARCHITEW6432 -eq 'ARM64')
$asset = if ($isArm) { 'crafthub-windows-aarch64.exe' } else { 'crafthub-windows-x86_64.exe' }
$base = "https://github.com/$repo/releases/latest/download"
$tmp = Join-Path ([IO.Path]::GetTempPath()) ('crafthub-' + [guid]::NewGuid())
New-Item -ItemType Directory -Path $tmp | Out-Null

try {
    Write-Host 'Downloading Craft Hub...'
    $exe = Join-Path $tmp $asset
    Invoke-WebRequest "$base/$asset" -OutFile $exe -UseBasicParsing
    Invoke-WebRequest "$base/SHA256SUMS.txt" -OutFile (Join-Path $tmp 'SHA256SUMS.txt') -UseBasicParsing

    $line = Get-Content (Join-Path $tmp 'SHA256SUMS.txt') | Where-Object { $_ -match "\s\*?$([regex]::Escape($asset))$" }
    $want = if ($line) { ($line -split '\s+')[0].ToLower() } else { '' }
    $got = (Get-FileHash $exe -Algorithm SHA256).Hash.ToLower()
    if ($want -ne $got) { throw 'Checksum check failed; not installing.' }

    $p = Start-Process -FilePath $exe -ArgumentList '--install-self' -Wait -PassThru
    if ($p.ExitCode -ne 0) { throw "Craft Hub failed to install (exit code $($p.ExitCode))." }

    Write-Host 'Craft Hub is installed. Find it in the Start Menu. Opening it now...'
    Start-Process (Join-Path $env:LOCALAPPDATA 'Programs\CraftHub\crafthub.exe')
}
finally {
    Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue
}
