# Prepend a VERIFY-log entry for current Cargo.toml version.
# Usage:
#   powershell -File scripts/new-verify-entry.ps1
#   powershell -File scripts/new-verify-entry.ps1 -Title "hotfix"
#   powershell -File scripts/new-verify-entry.ps1 -SkipIfExists
#
# Script should be saved as UTF-8 with BOM for Windows PowerShell 5.1 Chinese paths.

param(
    [string]$Title = "",
    [switch]$SkipIfExists
)

$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
Set-Location $Root

$Version = (Select-String -Path "Cargo.toml" -Pattern '^version\s*=\s*"([^"]+)"').Matches[0].Groups[1].Value
$Date = Get-Date -Format "yyyy-MM-dd"
$LogPath = Join-Path $Root "docs\product\VERIFY-log.md"
$ChecklistPath = Join-Path $Root "docs\product\VERIFY-checklist.md"

if (-not (Test-Path -LiteralPath $LogPath)) { throw "missing VERIFY-log.md" }
if (-not (Test-Path -LiteralPath $ChecklistPath)) { throw "missing VERIFY-checklist.md" }

$heading = if ($Title) {
    "## $Date · $Version · $Title"
} else {
    "## $Date · $Version"
}

# Read as UTF-8 (with or without BOM)
$utf8 = New-Object System.Text.UTF8Encoding $false
$existing = [System.IO.File]::ReadAllText($LogPath, $utf8)

if ($SkipIfExists -and $existing.Contains($heading)) {
    Write-Host "already has: $heading"
    exit 0
}

$nl = "`n"
$entry = @(
    $heading
    ""
    "### 命令"
    ""
    "- [ ] ``cargo test`` — （或 ``powershell -File scripts/verify.ps1``）"
    "- [ ] ``cargo build --release``"
    ""
    "### 手测"
    ""
    "完整基线：``docs/product/VERIFY-checklist.md``。将关键项复制到下方并勾选："
    ""
    "- [ ] （从 checklist 粘贴…）"
    ""
    "### 阻塞 / 备注"
    ""
    "- "
    ""
    "### REVIEW"
    ""
    "- PLAN：  "
    "- 遗留：  "
    ""
    "---"
    ""
) -join $nl

$idx = $existing.IndexOf("`n## ")
if ($idx -lt 0) {
    $idx = $existing.IndexOf("`r`n## ")
    if ($idx -ge 0) {
        # keep CR position: insert after the first \r of \r\n or after \n
        $insertAt = $idx + 1
        if ($existing[$idx] -eq "`r") { $insertAt = $idx + 2 }
        $newContent = $existing.Substring(0, $insertAt) + $entry + $existing.Substring($insertAt)
    } else {
        $newContent = $existing.TrimEnd() + $nl + $nl + $entry
    }
} else {
    $insertAt = $idx + 1
    $newContent = $existing.Substring(0, $insertAt) + $entry + $existing.Substring($insertAt)
}

# Write UTF-8 with BOM so WinPS / editors show Chinese correctly
$utf8Bom = New-Object System.Text.UTF8Encoding $true
[System.IO.File]::WriteAllText($LogPath, $newContent, $utf8Bom)
Write-Host "prepended: $heading"
Write-Host "  log: $LogPath"
