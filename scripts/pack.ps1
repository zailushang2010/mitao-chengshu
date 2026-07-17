# Build release and produce a portable zip under dist/
# Usage: pwsh -File scripts/pack.ps1

$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
Set-Location $Root

$Name = "蜜桃成熟"
$Version = (Select-String -Path "Cargo.toml" -Pattern '^version\s*=\s*"([^"]+)"').Matches[0].Groups[1].Value
$Stamp = Get-Date -Format "yyyyMMdd"
$PkgDirName = "${Name}-v${Version}-win64"
$DistRoot = Join-Path $Root "dist"
$PkgDir = Join-Path $DistRoot $PkgDirName
$ZipPath = Join-Path $DistRoot "${PkgDirName}.zip"
$ExeSrc = Join-Path $Root "target\release\${Name}.exe"

Write-Host "==> stop running instance (if any)"
Get-Process -Name $Name -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
Start-Sleep -Milliseconds 500

Write-Host "==> cargo test"
cargo test
if ($LASTEXITCODE -ne 0) { throw "tests failed" }

Write-Host "==> cargo build --release"
cargo build --release
if ($LASTEXITCODE -ne 0) { throw "release build failed" }
if (-not (Test-Path $ExeSrc)) { throw "missing $ExeSrc" }

Write-Host "==> assemble $PkgDir"
if (Test-Path $DistRoot) {
    # keep older zips; only replace this version folder
    if (Test-Path $PkgDir) { Remove-Item -Recurse -Force $PkgDir }
} else {
    New-Item -ItemType Directory -Path $DistRoot | Out-Null
}
New-Item -ItemType Directory -Path $PkgDir | Out-Null

Copy-Item $ExeSrc (Join-Path $PkgDir "${Name}.exe")
Copy-Item (Join-Path $Root "README.md") (Join-Path $PkgDir "README.md")

$Usage = @"
蜜桃成熟 v$Version
================

绿色便携：双击「蜜桃成熟.exe」即可。
首次运行会在同目录生成 config.json / history*.json。

快速开始
--------
【电影】
1. 顶部选「电影」
2. ⚙ 添加视频目录
3. 「随机预览」→ 可剔除不要的 → 「开启播放」
4. 「再来一批」只换预览，不自动播

【图片】
1. 顶部选「图片」
2. ⚙ 添加图片目录；选「幻灯片」或「平铺墙」
3. 「随机预览」→ 「开启幻灯」
4. 幻灯：空格暂停，←/→ 切换，Esc 结束
   平铺墙：点击放大，再点返回，Esc 结束

依赖
----
- 电影模式需要本机安装 PotPlayer（设置里可指定路径）
- 图片模式为内置播放，不依赖 PotPlayer

打包日期：$Stamp
"@
Set-Content -Path (Join-Path $PkgDir "使用说明.txt") -Value $Usage -Encoding UTF8

Write-Host "==> zip $ZipPath"
if (Test-Path $ZipPath) { Remove-Item -Force $ZipPath }
Compress-Archive -Path $PkgDir -DestinationPath $ZipPath -CompressionLevel Optimal

$ExeInfo = Get-Item (Join-Path $PkgDir "${Name}.exe")
$ZipInfo = Get-Item $ZipPath
Write-Host ""
Write-Host "OK  package folder: $PkgDir"
Write-Host "OK  zip:            $ZipPath"
Write-Host "    exe size:       $([math]::Round($ExeInfo.Length/1MB, 2)) MB"
Write-Host "    zip size:       $([math]::Round($ZipInfo.Length/1MB, 2)) MB"
