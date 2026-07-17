# suijiPotPlayer 实现计划

> **面向 AI 代理的工作者：** 必需子技能：使用 superpowers:subagent-driven-development（推荐）或 superpowers:executing-plans 逐任务实现此计划。步骤使用复选框（`- [ ]`）语法来跟踪进度。

**目标：** 构建 Windows 上可双击运行的 Rust 桌面工具：纸感杂志风 UI，从单一片库随机开 5–10 部电影到多个 PotPlayer，并在工作区网格平铺；支持开启 / 再来一轮 / 关闭本轮。

**架构：** egui/eframe 负责 UI；SessionOrchestrator 状态机串联 Library 扫描、Picker 随机、PotPlayerHost 启停、WindowTiler 摆窗与 History 防重复；配置与历史以 JSON 落在 exe 旁。

**技术栈：** Rust、eframe/egui、serde/serde_json、rand、windows crate

**规格：** `docs/superpowers/specs/2026-07-17-suiji-potplayer-design.md`

---

## 文件结构

| 路径 | 职责 |
|------|------|
| `Cargo.toml` | 依赖与包元数据 |
| `src/main.rs` | 入口、单实例、启动 eframe |
| `src/config.rs` | Config 结构、默认值、读写 `config.json` |
| `src/history.rs` | 最近播放路径读写 `history.json` |
| `src/library.rs` | 递归扫库、扩展名过滤、索引缓存 |
| `src/picker.rs` | 随机抽取 N 部，防重复逻辑 |
| `src/potplayer.rs` | 探测路径、启动 `/new`、记 PID、结束进程、音量尽力 |
| `src/tiler.rs` | 工作区矩形、网格行列、MoveWindow |
| `src/session.rs` | 状态机与编排 |
| `src/app/mod.rs` | egui App：主界面、设置、主题色 |
| `src/app/theme.rs` | 纸感杂志颜色与间距常量 |
| `README.md` | 使用说明 |
| `tests/` 或 `src` 内 `#[cfg(test)]` | 库过滤、网格计算、picker 边界 |

---

### 任务 1：脚手架与 Config

**文件：**
- 创建：`Cargo.toml`、`src/main.rs`、`src/config.rs`

- [ ] **步骤 1：** `cargo init` 于项目根（若已有则跳过），加入依赖：`eframe`、`egui`、`serde`、`serde_json`、`rand`、`windows`（features: Win32_Foundation, Win32_UI_WindowsAndMessaging, Win32_System_Threading, Win32_System_SystemInformation, Win32_Security 等按编译补齐）

- [ ] **步骤 2：** 实现 `Config` 与 `load_or_default` / `save`，路径为 exe 目录（开发时用 `CARGO_MANIFEST_DIR` 或 current_dir 回退）

- [ ] **步骤 3：** 单元测试：损坏 JSON 时回退默认；`default_count` 夹在 min/max

- [ ] **步骤 4：** Commit：`feat: scaffold config module`

---

### 任务 2：Library + Picker + History

**文件：**
- 创建：`src/library.rs`、`src/picker.rs`、`src/history.rs`

- [ ] **步骤 1：** `Library::scan(path, extensions) -> Vec<PathBuf>`；跳过不可读目录

- [ ] **步骤 2：** 测试：临时目录造 `.mkv`/`.txt`/子目录，断言只收视频

- [ ] **步骤 3：** `History` 读写；`Picker::pick(library, n, avoid_recent, history) -> Vec<PathBuf>`

- [ ] **步骤 4：** 测试：n 大于库大小时返回全部；avoid 耗尽时回退

- [ ] **步骤 5：** Commit：`feat: library scan, history, random picker`

---

### 任务 3：PotPlayerHost + WindowTiler

**文件：**
- 创建：`src/potplayer.rs`、`src/tiler.rs`

- [ ] **步骤 1：** `resolve_potplayer_path(config) -> Option<PathBuf>` 探测

- [ ] **步骤 2：** `launch(path) -> Result<u32>` 返回 PID；`kill_pids(&[u32])`

- [ ] **步骤 3：** `work_area() -> RECT`；`grid_layout(n, area) -> Vec<RECT>`；`tile_windows(hwnds, rects)`

- [ ] **步骤 4：** 测试：`grid_layout` 对 n=6 得到 6 个不重叠矩形且并集在 area 内

- [ ] **步骤 5：** Commit：`feat: potplayer host and window tiler`

---

### 任务 4：SessionOrchestrator

**文件：**
- 创建：`src/session.rs`

- [ ] **步骤 1：** 定义 `SessionState` 枚举与 `SessionController`

- [ ] **步骤 2：** `start(n)` / `stop()` / `reroll(n)`；异步或后台线程避免卡 UI

- [ ] **步骤 3：** 启动后轮询 HWND 再 tile；写 history

- [ ] **步骤 4：** Commit：`feat: session orchestrator state machine`

---

### 任务 5：纸感 egui UI

**文件：**
- 创建：`src/app/mod.rs`、`src/app/theme.rs`
- 修改：`src/main.rs`

- [ ] **步骤 1：** 主题色与 Visuals 定制

- [ ] **步骤 2：** 主界面：数量、音量、防重复、三按钮、状态、索引数、本轮文件名

- [ ] **步骤 3：** 片库设置：选文件夹（`rfd` crate 文件对话框）、重扫、PotPlayer 路径

- [ ] **步骤 4：** 单实例互斥；窗口尺寸约 420×560

- [ ] **步骤 5：** Commit：`feat: magazine-style egui shell`

---

### 任务 6：联调、README、Release

**文件：**
- 创建：`README.md`
- 修改：按需修 bug

- [ ] **步骤 1：** 本机联调：真实片库 + PotPlayer 开/关/再来一轮

- [ ] **步骤 2：** README：构建命令、配置项、使用步骤

- [ ] **步骤 3：** `cargo build --release` 验证

- [ ] **步骤 4：** Commit：`docs: readme and release notes`

---

## 规格覆盖检查

| 规格章节 | 任务 |
|----------|------|
| 配置 | 1 |
| 扫库/随机/历史 | 2 |
| PotPlayer/平铺 | 3 |
| 会话状态机 | 4 |
| UI 纸感 | 5 |
| 错误处理/验收 | 4–6 |
| 音量尽力 | 3、6 |
