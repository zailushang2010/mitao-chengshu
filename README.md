# 蜜桃成熟

本地电影太多、选择困难时用的小工具：从**一个或多个片库目录**随机抽出 5–10 部，用 **PotPlayer 多实例**在屏幕**工作区网格**（避开任务栏）平铺播放。

纸感杂志风界面 · Rust · 开 / 关 / 再来一轮 · 托盘 · 缩略图预览

## 功能

- 支持**多个片库目录**同时递归扫描
- 可调本轮数量（默认 5–10）、统一音量偏好、避开最近播放
- **开启本轮** / **再来一轮** / **关闭本轮**（只结束本工具拉起的进程）
- 窗口自动网格平铺，不遮挡任务栏
- 右上角图标：片库设置 / 重新扫描 / 最小化到托盘
- 默认点 **关闭(X) = 退出程序**；托盘仅通过图标手动收起（设置里可改）
- 配置与历史写在 exe 同目录：`config.json`、`history.json`

## 构建

```bash
cargo build --release
```

产物：`target/release/蜜桃成熟.exe`  
图标：`src/icon.ico`（嵌入 exe，并用于窗口 / 托盘）

## 使用

1. 运行 `蜜桃成熟.exe`
2. 右上角 ⚙ → **添加文件夹…**（可多个）→ 等待索引
3. 调整数量 / 音量 / 防重复（可选）
4. 点击 **开启本轮**

## 配置说明（`config.json`）

| 字段 | 含义 |
|------|------|
| `library_paths` | 电影目录列表（可多个） |
| `library_path` | 兼容旧单路径，会并入 `library_paths` |
| `default_count` | 默认开启数量 |
| `count_min` / `count_max` | 数量调节范围（设置里可改，绝对 1–32） |
| `volume_percent` | 音量偏好（0–100） |
| `avoid_recent` | 是否避开最近播放 |
| `recent_history_size` | 历史条数上限 |
| `potplayer_path` | 空则自动探测 |
| `video_extensions` | 识别的后缀列表 |
| `close_session_on_exit` | 退出程序时是否关掉本轮 PotPlayer |
| `minimize_to_tray` | 点关闭(X)时是否进托盘（默认 false） |

## 开发

```bash
cargo test
cargo run --release
```
