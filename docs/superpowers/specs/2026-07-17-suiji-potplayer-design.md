# suijiPotPlayer 设计规格

**日期：** 2026-07-17  
**状态：** 已确认  
**定位：** Windows 本地片库随机多开工具——Rust 单 exe + 纸感杂志风 UI + PotPlayer 网格平铺播放

---

## 1. 背景与目标

用户电脑中有大量电影，每次打开片库都面临选择困难。本工具每次从指定主目录随机选取 5–10 部影片，用多个 PotPlayer 实例在屏幕工作区（避开任务栏）网格平铺并自动播放，从「选哪部」变成「扫一眼再决定」。

### 1.1 成功标准

1. 配置好片库后，点击「开启本轮」约 5 秒内出现 N 个 PotPlayer 窗口并开始播放。
2. 窗口呈均匀网格，不遮挡 Windows 任务栏。
3. 「关闭本轮」仅结束本工具启动的 PotPlayer 实例，不影响用户手动打开的实例。
4. 「再来一轮」先关闭本轮再重新抽片；防重复开启时尽量不与近期历史重复。
5. UI 符合纸感杂志视觉规范，主流程在三步内完成（选库 → 调参 → 开启）。

### 1.2 非目标（首版不做）

- 多片库并行、网络刮削海报/评分
- 多显示器复杂布局策略（首版仅主显示器工作区）
- 云同步、账号体系
- 替代 PotPlayer 的内嵌解码播放

---

## 2. 已确认的产品决策

| 决策点 | 选择 |
|--------|------|
| 用途 | 预览选片 + 可保持多开同时观看 |
| 片库 | 单一主目录，递归子目录 |
| 声音 | 全部有声；统一默认较低音量 |
| 数量 | 默认可配置；界面可在 5–10 间调节 |
| 布局 | 自动网格，使用工作区矩形（避开任务栏） |
| 启动形态 | 优雅 GUI 控制台（非纯托盘/纯 CLI） |
| 防重复 | 默认开启，可关闭 |
| 技术 | Rust 单 exe 启动开关 |
| 视觉 | 纸感杂志（暖米白 + 衬线标题 + 克制动效） |

---

## 3. 架构

### 3.1 分层

```
UI (eframe/egui, 纸感杂志)
        │ 命令 / 状态订阅
SessionOrchestrator
  开本轮 · 再来一轮 · 关本轮 · 状态机
        │
   ┌────┼────────────┬─────────────┐
   ▼    ▼            ▼             ▼
Library Picker   PotPlayerHost  WindowTiler
History ConfigStore
```

### 3.2 模块职责

| 模块 | 职责 | 依赖 |
|------|------|------|
| **ConfigStore** | 读写与 exe 同目录的 `config.json`；提供默认值合并 | serde, 文件系统 |
| **Library** | 递归扫描 `library_path`；按扩展名过滤；缓存文件列表与数量 | Config |
| **Picker** | 从库中均匀随机抽 N 部；可选排除历史集合 | Library, History, rand |
| **History** | 持久化最近播放路径列表；受 `recent_history_size` 限制 | 本地 JSON 或并入 config 侧车文件 |
| **PotPlayerHost** | 解析/探测 PotPlayer 路径；以 `/new` 启动；记录 PID；尝试设音量；按 PID 批量结束 | windows API, Config |
| **WindowTiler** | 获取主屏工作区；按 N 计算行列与矩形；对会话窗口 `MoveWindow`/`SetWindowPos` | windows API |
| **SessionOrchestrator** | 状态机与并发控制；串联抽片→启动→平铺→收尾 | 上述全部 |
| **UI** | 展示状态、参数、本轮列表预览；发起命令 | Orchestrator, Config |

### 3.3 会话状态机

```
Idle ──开启──► Starting ──成功──► Playing
                  │ 失败              │
                  ▼                   │ 关闭 / 再来一轮(先停)
                Idle ◄── Stopping ◄──┘
```

- **Idle：** 无本工具管理的播放会话（或已全部关闭）。
- **Starting：** 抽片与拉起进程中；UI 禁用重复「开启」，可显示进度文案。
- **Playing：** 持有本轮 `Vec<SessionItem { path, pid, optional hwnd }>`。
- **Stopping：** 正在结束进程；完成后回到 Idle。

「再来一轮」= `Stopping` → 成功后立即进入新的 `Starting`。

单实例：进程级命名互斥量，避免两个控制台争抢同一会话语义。

---

## 4. 用户流程

### 4.1 首次使用

1. 启动 exe，状态「就绪」。
2. 若 `library_path` 为空或无效 → 提示并打开「片库设置」。
3. 用户选择主目录 → 后台扫描 → 显示「已索引 N 部」。
4. 可选调整数量、音量、防重复。
5. 点击「开启本轮」。

### 4.2 日常

- **开启本轮：** Idle → 抽片 → 多开 PotPlayer → 平铺 → Playing。
- **再来一轮：** Playing → 关闭本轮 PID → 重新抽片开启。
- **关闭本轮：** Playing → 仅杀会话 PID → Idle。
- **退出应用：** 默认**不**关闭本轮播放（`close_session_on_exit: false`）；设置中可改为退出时一并关闭。

### 4.3 主界面结构（纸感杂志）

1. **顶栏：** 品牌小字 + 标题「今日片单」+ 状态胶囊（就绪 / 启动中 / 播放中 / 关闭中 / 错误）。
2. **路径行：** 片库路径摘要 + 已索引数量。
3. **调节区：** 本轮数量（− / +，夹在 count_min–count_max）、统一音量滑条、避开最近播放开关。
4. **本轮预览：** 示意网格 + 开启后显示本轮文件名（截断）。
5. **操作区：** 主按钮「开启本轮」；次按钮「再来一轮」「关闭本轮」；底链「片库设置 · 关于」。

窗口约 420×560，固定宽度为主；设置可用模态层或内嵌页。

---

## 5. 配置

路径：与可执行文件同目录的 `config.json`。缺失时写入默认值。

```json
{
  "library_path": "",
  "default_count": 6,
  "count_min": 5,
  "count_max": 10,
  "volume_percent": 28,
  "avoid_recent": true,
  "recent_history_size": 40,
  "potplayer_path": "",
  "video_extensions": [
    ".mkv", ".mp4", ".avi", ".ts", ".m2ts",
    ".wmv", ".mov", ".flv", ".webm"
  ],
  "close_session_on_exit": false
}
```

### 5.1 字段语义

| 字段 | 说明 |
|------|------|
| `library_path` | 电影主目录，递归扫描 |
| `default_count` | 界面默认数量，启动时载入 |
| `count_min` / `count_max` | 界面调节上下限（产品约定 5–10） |
| `volume_percent` | 0–100，尝试应用到每个实例 |
| `avoid_recent` | 是否排除历史 |
| `recent_history_size` | 历史最多保留条数 |
| `potplayer_path` | 空则自动探测 |
| `video_extensions` | 小写比较；扫描时规范化 |
| `close_session_on_exit` | 退出 UI 是否结束本轮进程 |

历史列表可存 `history.json`（路径列表），与 config 分离以免频繁重写大配置。

### 5.2 PotPlayer 探测顺序

1. `config.potplayer_path`（若非空且文件存在）
2. `C:\Program Files\DAUM\PotPlayer\PotPlayerMini64.exe`
3. `C:\Program Files\DAUM\PotPlayer\PotPlayerMini.exe`
4. `C:\Program Files (x86)\DAUM\PotPlayer\PotPlayerMini.exe`
5. 失败 → UI 要求用户浏览指定

---

## 6. 库扫描与随机

### 6.1 扫描

- 递归遍历 `library_path`。
- 仅文件、扩展名在 `video_extensions` 内（大小写不敏感）。
- 跳过无法访问的目录（记录日志，不中断整体）。
- 结果去重（同一规范路径一次）。
- 扫描可在后台线程；UI 显示「索引中…」与完成后的数量。
- 可在设置中提供「重新扫描」。

### 6.2 随机抽取

- 目标数量 `n = clamp(ui_count, count_min, count_max)`。
- 候选集 = 全库；若 `avoid_recent` 且历史非空，先去掉仍存在于库中的历史路径。
- 若过滤后数量 `< n`：放宽为全库（仍尽量优先未在历史中的），再不足则 `n = 候选数`，UI 提示「片源不足，已开 k 部」。
- 无放回均匀随机。
- 成功启动后将本轮路径追加进 History，并裁剪到 `recent_history_size`。

---

## 7. PotPlayer 与窗口平铺

### 7.1 启动

对每个路径：

```text
"<potplayer>" "<absolute-path>" /new
```

- 使用绝对路径；路径含空格时由进程 API 正确引用。
- 记录每个子进程 PID；尽可能枚举到主窗口 HWND（按 PID 匹配，短重试）。

### 7.2 音量

实现阶段按可用性优先尝试：

1. 启动参数或 PotPlayer 支持的命令行音量选项（以实测为准）。
2. 否则启动后对窗口发送音量相关消息/模拟键（若稳定）。
3. 均不可靠时：保留配置项与 UI 滑条，文档说明「请在 PotPlayer 默认音量中设置」，不阻断开播。

**验收：** 不因音量失败而中止会话；至少保证开播与平铺。

### 7.3 工作区网格

- 使用 `SystemParametersInfo(SPI_GETWORKAREA)` 或等价 API 获取主显示器工作区（排除任务栏）。
- 行列启发式（首版固定表 + 通用公式）：
  - 优先接近正方形的网格；
  - 示例：5→2×3，6→2×3，7→2×4，8→2×4，9→3×3，10→2×5。
- 将工作区均分为 `rows × cols` 单元格；窗口按顺序填入；多余格子留空。
- `SetWindowPos` / `MoveWindow`，可去最大化后移动。
- 窗口未就绪：间隔约 200ms 重试，最多约 10 次；超时则该窗跳过定位，进程保留。

### 7.4 关闭

- 仅对会话内 PID 调用终止（优先 `TerminateProcess` 或先尝试关闭主窗口再超时强杀，实现选稳妥策略）。
- 清理会话列表；状态回 Idle。
- **禁止**按窗口标题全局杀所有 PotPlayer。

---

## 8. UI 视觉规范

| 令牌 | 值 |
|------|-----|
| 背景 | `#F7F3EC` |
| 主文字 | `#1C1917` |
| 次要文字 | `#78716C` |
| 弱化 | `#A8A29E` |
| 边框/分割 | `#E7E0D6` / `#D6D3D1` |
| 主按钮底 | `#1C1917`，字 `#FAFAF9` |
| 次按钮 | 透明底 + `#A8A29E` 描边 |
| 标题字体 | 衬线优先（Georgia / 系统衬线），中文 fallback |
| 控件字体 | 系统无衬线（Segoe UI 等） |
| 动效 | 状态与按钮反馈克制；无粒子/强光效 |

风格关键词：纸感、杂志、安静、字距略宽的主按钮文案。

---

## 9. 技术选型

| 用途 | 选择 |
|------|------|
| 语言 | Rust（2021 edition 或更新） |
| GUI | `eframe` + `egui` |
| 序列化 | `serde` / `serde_json` |
| 随机 | `rand` |
| Windows API | `windows` crate（进程、窗口、工作区） |
| 发布 | `cargo build --release` 单 exe；可选压缩 |

不引入 WebView（Tauri）作为首版，以控制体积与皮肤一致性。

---

## 10. 错误处理

| 场景 | 行为 |
|------|------|
| 片库路径无效或为空 | 状态错误文案；引导片库设置 |
| 库中无视频 | 禁止开启；提示添加影片或检查扩展名 |
| 视频数 < 请求 N | 开满可用数量并提示 |
| 找不到 PotPlayer | 设置页指定路径 |
| 单个文件启动失败 | 跳过，继续其余；结束后摘要 |
| 平铺部分失败 | 不中断播放 |
| 配置 JSON 损坏 | 备份坏文件，恢复默认并提示 |

---

## 11. 测试要点

1. **库扫描：** 嵌套目录、大小写扩展名、空目录、无权限子目录。
2. **随机与历史：** 防重复开关开/关；历史耗尽后的回退。
3. **会话：** 开 → 关 → 再开；再来一轮；重复点击开启的防抖。
4. **窗口：** 不同 N 的网格；任务栏在底/侧时工作区正确。
5. **隔离：** 手动开一个 PotPlayer 后「关闭本轮」不得误杀。
6. **配置：** 删除 config 后默认生成；修改数量/音量持久化。

---

## 12. 仓库与交付结构（建议）

```
suijiPotPlayer/
  Cargo.toml
  src/
    main.rs
    app/          # egui 应用与页面
    config.rs
    library.rs
    picker.rs
    history.rs
    potplayer.rs
    tiler.rs
    session.rs
  docs/superpowers/specs/
    2026-07-17-suiji-potplayer-design.md
  README.md
```

---

## 13. 实现优先级

1. Config + Library 扫描 + 单元测试级路径过滤  
2. PotPlayerHost 启停 + PID 会话  
3. WindowTiler 工作区网格  
4. SessionOrchestrator 状态机  
5. egui 纸感主界面与设置  
6. History 防重复 + 音量尽力而为  
7. Release 构建与 README 使用说明  

---

## 14. 规格自检记录

- 无 TODO/待定占位影响实现。  
- 音量策略允许降级，不阻断主路径。  
- 范围聚焦单一主目录 + 主屏工作区 + PotPlayer，未膨胀。  
- 「关闭」语义明确为会话 PID，非全局杀进程。
