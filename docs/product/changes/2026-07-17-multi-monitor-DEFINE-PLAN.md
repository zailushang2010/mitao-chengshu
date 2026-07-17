# CHANGE: P2-2 多显示器选择工作区

## DEFINE

### 问题

多屏用户平铺总落在主屏工作区，副屏使用场景无法指定。

### 范围内

- 枚举当前显示器（名称/序号 + 分辨率摘要）
- 设置中选择「平铺工作区」所在显示器（默认：主屏 / 系统主工作区）
- `session` 平铺与几何守护均使用所选屏的 **工作区**（排除任务栏）
- 配置持久化；所选屏消失时回退主工作区并 toast（可选安静回退）

### 范围外

- 跨多屏拼接成一块超大网格
- 每部影片单独指定显示器
- 控制面板本身钉在某屏（由用户拖窗口）

### 验收

- [ ] 双屏：设置选副屏 → 开启播放 → N 窗落在副屏工作区
- [ ] 选主屏行为与 0.6 主区平铺一致
- [ ] 拔掉所选屏后再次开播：不崩溃，落到可用工作区
- [ ] `cargo test` + `cargo build --release`

## PLAN

| # | 任务 | 文件 |
|---|------|------|
| 1 | `tiler::list_monitors()` + `work_area_for(index)` | `tiler.rs` |
| 2 | `Config.tile_monitor_index: i32`（-1=主工作区 SPI） | `config.rs` |
| 3 | session 取 area 处改用 config | `session.rs` |
| 4 | 设置 UI：下拉/列表 | `settings.rs` |
| 5 | 单测：rows_cols 无关；可选 mock 列表结构 | |
| 6 | VERIFY 手测双屏 | |

### 方案取舍

- **A** `SPI_GETWORKAREA` only（现状）— 不够  
- **B** EnumDisplayMonitors + rcWork + 配置 index — **采用**  
- **C** 按鼠标所在屏 — 不可预测，不做默认

### 风险

- 显示器顺序随驱动变化 → 用 index + 保存时名称，加载时优先名匹配再 index  
- 简化 v1：只存 `tile_monitor_index: i32`，-1 表示「主工作区（系统）」；≥0 为枚举序

## BUILD

- `tiler::list_monitors` / `resolve_work_area`
- `Config.tile_monitor_index`（默认 -1）
- session 开播/重铺取 area
- 设置 ComboBox

## VERIFY / REVIEW

见 `docs/product/VERIFY-log.md` · 0.6.1 条目；P2-2 done。
