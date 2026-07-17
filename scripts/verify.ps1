# Run automated VERIFY gates for mitao_chengshu / 蜜桃成熟.
# Usage: powershell -File scripts/verify.ps1 [-SkipBuild] [-StopRunning]
#
# Hand tests: docs/product/VERIFY-checklist.md
# Log entry:  powershell -File scripts/new-verify-entry.ps1 -Title "..."

param(
    [switch]$SkipBuild,
    [switch]$StopRunning
)

$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
Set-Location $Root

$Version = (Select-String -Path "Cargo.toml" -Pattern '^version\s*=\s*"([^"]+)"').Matches[0].Groups[1].Value
$ReleaseDir = Join-Path $Root "target\release"

Write-Host "=== VERIFY v$Version ===" -ForegroundColor Cyan

if ($StopRunning) {
    Write-Host "==> stop running instances (if any)"
    # Match by path under this repo when possible
    Get-Process -ErrorAction SilentlyContinue |
        Where-Object {
            try {
                $_.Path -and $_.Path.StartsWith($Root, [System.StringComparison]::OrdinalIgnoreCase)
            } catch { $false }
        } |
        Stop-Process -Force -ErrorAction SilentlyContinue
    Start-Sleep -Milliseconds 600
}

# Use cmd so cargo stderr warnings do not become PS terminating errors
Write-Host "==> cargo test"
cmd /c "cargo test"
if ($LASTEXITCODE -ne 0) { throw "cargo test failed" }

if (-not $SkipBuild) {
    Write-Host "==> cargo build --release"
    cmd /c "cargo build --release"
    if ($LASTEXITCODE -ne 0) {
        throw "cargo build --release failed (close the app if Access denied)"
    }
    $exe = Get-ChildItem -LiteralPath $ReleaseDir -Filter "*.exe" -File -ErrorAction SilentlyContinue |
        Where-Object { $_.Name -notmatch '^\.' } |
        Sort-Object LastWriteTime -Descending |
        Select-Object -First 1
    if (-not $exe) { throw "no .exe under $ReleaseDir" }
    Write-Host ("    ok: {0} ({1} bytes)" -f $exe.FullName, $exe.Length)
}

Write-Host ""
Write-Host "=== Automated gates OK ===" -ForegroundColor Green
Write-Host "Next: docs/product/VERIFY-checklist.md"
Write-Host "      powershell -File scripts/new-verify-entry.ps1 -Title `"...`""
Write-Host "      powershell -File scripts/pack.ps1"
