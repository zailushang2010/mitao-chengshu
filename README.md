# 蜜桃成熟

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](./LICENSE)
[![Platform](https://img.shields.io/badge/platform-Windows-blue.svg)](#系统要求)

本地片库 / 图库太多、选择困难时用的小工具：

- **电影**：随机预览 → 确认后 **多开 PotPlayer** 并网格平铺  
- **图片**：随机预览 → **幻灯片** 或 **平铺墙**  

纸感杂志风 · 工作台布局 · Rust 单 exe · 托盘 · 多片库 · **MIT 开源**

> 当前版本 **0.6.2**。开发流程：DEFINE → PLAN → BUILD → VERIFY → REVIEW  
> 见 [`AGENTS.md`](./AGENTS.md) · [`docs/process/lifecycle.md`](./docs/process/lifecycle.md) · [`docs/product/DEFINE.md`](./docs/product/DEFINE.md)

---

## 功能概览

| | 电影 | 图片 |
|--|------|------|
| 片库 | 多目录扫描视频 | 多目录扫描图片（独立配置） |
| 流程 | 随机预览 → 剔除 / 拉黑 → 开启 | 相同 |
| 开启后 | PotPlayer 多实例网格平铺 | 幻灯片 / 平铺墙 |
| 历史 | `history.json` | `history_images.json` |

其它：

- 工作台：可折叠操作栏（开合会记住）、顶栏调本轮数量、主区预览  
- 避开最近播放；拉黑永久排除（设置可移出）  
- 电影：置前 / 独播 / 关闭；播放时可选面板置顶  
- 电影播放中可切图片并存；可关后台电影  
- 平铺工作区可选显示器；单实例二次启动唤醒  
- 索引缓存 + 可取消；关闭(X) 可退出或进托盘  

---

## 系统要求

- **Windows**（当前仅此平台）  
- 电影模式：已安装 [PotPlayer](https://potplayer.daum.net/)（或在设置中指定 `PotPlayerMini64.exe` 路径）  
- 可选：系统 `ffmpeg` 用于无旁路封面时的缩略图抽帧  

本软件 **不捆绑、不修改** PotPlayer；播放器为独立第三方产品，请遵守其许可与使用条款。

---

## 使用

### 电影

1. 顶部点 **电影**  
2. 设置（齿轮）添加视频目录  
3. **随机预览** → 可剔除 / 拉黑 → **开启播放**  
4. **再来一批** 只换预览，不自动播  

### 图片

1. 顶部点 **图片**  
2. 添加图片目录；选择 **幻灯片** 或 **平铺墙**  
3. **随机预览** → **开启幻灯** / 开墙  
4. 幻灯：空格暂停，左右切换，Esc 结束；墙：点击放大，Esc 结束  

### 电影 + 图片并存

- 电影 **播放中** 切到 **图片**：PotPlayer 继续播，应用内可幻灯/墙  
- 可 **关掉电影**，或切回电影后用 **关闭本轮**  

---

## 构建 / 打包

需安装 [Rust](https://rustup.rs/)（稳定版）与 Windows 构建环境。

```powershell
cargo test
cargo build --release
```

产物：`target/release/蜜桃成熟.exe`

一键测试 + release + zip（推荐）：

```powershell
powershell -File scripts/pack.ps1
```

输出示例：

- `dist/蜜桃成熟-v0.6.2-win64/`  
- `dist/蜜桃成熟-v0.6.2-win64.zip`  

验证门禁：

```powershell
powershell -File scripts/verify.ps1 -StopRunning
```

手测基线：[`docs/product/VERIFY-checklist.md`](./docs/product/VERIFY-checklist.md)

---

## 配置

与 **exe 同目录** 的 `config.json`（首次运行自动生成；**请勿把含私人路径的配置提交到 Git**）。

常用字段：

| 字段 | 含义 |
|------|------|
| `library_paths` / `image_library_paths` | 电影 / 图片目录 |
| `default_count` 等 | 本轮数量（电影/图片独立） |
| `avoid_recent` | 避开最近 |
| `potplayer_path` | 空则自动探测 |
| `tile_monitor_index` | 平铺显示器：`-1` 主工作区，`0..` 枚举序号 |
| `workbench_sidebar_open` | 操作栏是否展开 |
| `minimize_to_tray` | 点 X 是否进托盘 |
| `close_session_on_exit` | 退出时是否关闭本轮 PotPlayer |

其它运行时文件（均本地、勿提交）：`history*.json`、`blacklist*.json`、`index_cache/`、`thumbs/`。

---

## 开发与贡献

欢迎 Issue / PR。请先阅读：

- [`CONTRIBUTING.md`](./CONTRIBUTING.md)  
- 产品定义：[`docs/product/DEFINE.md`](./docs/product/DEFINE.md)  
- 滚动计划：[`docs/product/PLAN.md`](./docs/product/PLAN.md)  

本地可选片库冒烟（不会进 CI）：

```powershell
$env:MITAO_TEST_LIBRARY="D:\Videos"
cargo test scan_local_library_env -- --ignored --nocapture
```

---

## 许可证

本项目以 **[MIT License](./LICENSE)** 发布。

第三方：egui / eframe、PotPlayer（用户自备）、可选 ffmpeg 等，各依其自身许可。

---

## 免责声明

- 软件按「现状」提供，作者不对数据丢失、误关进程等承担责任。  
- 请仅用于你有权访问的本地媒体；遵守所在地法律法规与版权规定。  
- 与 PotPlayer 的兼容性因版本/皮肤而异；平铺依赖窗口几何纠正，极端情况请用应用内「重新平铺」。  

---

## English (brief)

**MiTao ChengShu** is a small Windows tool that randomly picks videos or images from local libraries, lets you preview and trim the list, then either launches multiple **PotPlayer** windows in a grid, or plays images as a slideshow / wall. MIT licensed. Chinese UI and docs are primary; contributions welcome.
