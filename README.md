# 蜜桃成熟

本地媒体太多、选择困难时用的小工具：

- **电影**：随机预览 → 确认后多开 PotPlayer 平铺播放  
- **图片**：随机预览 → 确认后 **幻灯片** 或 **平铺墙**

纸感杂志风 · Rust 单 exe · 托盘 · 多片库

## 功能概览

| | 电影 | 图片 |
|--|------|------|
| 片库 | 多目录扫描视频 | 多目录扫描图片（独立配置） |
| 流程 | 随机预览 → 剔除 → 开启播放 | 相同 |
| 开启后 | PotPlayer 多实例网格 | 幻灯片 / 平铺墙 |
| 历史 | `history.json` | `history_images.json` |

其它：

- 数量上下限可配置（1–32）
- 避开最近播放
- 预览剔除、再来一批（不自动播放）
- 电影：单部置前 / 独播 / 关闭；播放时可选置顶
- 关闭(X)默认退出；可改托盘

## 使用

### 电影

1. 顶部点 **电影**
2. ⚙ 添加视频目录
3. **随机预览** → 可剔除 → **开启播放**
4. **再来一批** 只换预览，不自动播

### 图片

1. 顶部点 **图片**
2. ⚙ 添加图片目录；选择 **幻灯片** 或 **平铺墙**；幻灯可设间隔秒数
3. **随机预览** → **开启幻灯**
4. 幻灯：空格暂停，←/→ 切换，Esc 结束  
   平铺墙：点击放大，再点返回，Esc 结束

## 构建

```bash
cargo build --release
```

产物：`target/release/蜜桃成熟.exe`  
图标：`src/icon.ico`

## 配置（`config.json`，与 exe 同目录）

| 字段 | 含义 |
|------|------|
| `media_mode` | `movie` / `image` |
| `library_paths` | 电影目录 |
| `image_library_paths` | 图片目录 |
| `default_count` / `count_min` / `count_max` | 电影本轮数量（与图片独立） |
| `image_default_count` / `image_count_min` / `image_count_max` | 图片本轮数量 |
| `image_play_style` | `slideshow` / `wall` |
| `slideshow_interval_secs` | 幻灯间隔 1–60 |
| `avoid_recent` | 避开最近 |
| `potplayer_path` | 空则自动探测 |
| `video_extensions` / `image_extensions` | 后缀 |
| `minimize_to_tray` | X 是否进托盘 |
| `close_session_on_exit` | 退出时是否关本轮 PotPlayer |

## 开发

```bash
cargo test
cargo run --release
```
