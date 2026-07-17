# suijiPotPlayer · 今日片单

本地电影太多、选择困难时用的小工具：从**单一片库目录**随机抽出 5–10 部，用 **PotPlayer 多实例**在屏幕**工作区网格**（避开任务栏）平铺播放。

纸感杂志风界面 · Rust 单文件 · 开 / 关 / 再来一轮

## 功能

- 递归扫描片库，按扩展名识别视频
- 可调本轮数量（默认 5–10）、统一音量偏好、避开最近播放
- **开启本轮** / **再来一轮** / **关闭本轮**（只结束本工具拉起的进程）
- 窗口自动网格平铺，不遮挡任务栏
- 配置与历史写在 exe 同目录：`config.json`、`history.json`

## 环境

- Windows 10/11
- 已安装 [PotPlayer](https://potplayer.daum.net/)
- 构建需要 [Rust](https://rustup.rs/)（仅开发时）

## 构建

```bash
cargo build --release
```

产物：`target/release/suiji_potplayer.exe`  
可复制到任意目录使用（建议固定目录，便于保留配置）。

## 使用

1. 运行 `suiji_potplayer.exe`
2. 打开 **片库设置** → 选择电影主目录 → 等待索引
3. 调整数量 / 音量 / 防重复（可选）
4. 点击 **开启本轮**
5. 看腻了点 **再来一轮** 或 **关闭本轮**

首次若未自动找到 PotPlayer，在设置中浏览指定 `PotPlayerMini64.exe`。

## 配置说明（`config.json`）

| 字段 | 含义 |
|------|------|
| `library_path` | 电影主目录 |
| `default_count` | 默认开启数量 |
| `count_min` / `count_max` | 数量调节范围 |
| `volume_percent` | 音量偏好（0–100；播放器侧以实际支持为准） |
| `avoid_recent` | 是否避开最近播放 |
| `recent_history_size` | 历史条数上限 |
| `potplayer_path` | 空则自动探测 |
| `video_extensions` | 识别的后缀列表 |
| `close_session_on_exit` | 退出程序时是否关掉本轮 PotPlayer |

## 开发

```bash
cargo test
cargo run
```

设计规格：`docs/superpowers/specs/2026-07-17-suiji-potplayer-design.md`  
实现计划：`docs/superpowers/plans/2026-07-17-suiji-potplayer.md`

## 许可

自用工具，按需修改。
