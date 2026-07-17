# VERIFY 日志

按 `docs/process/lifecycle.md` 追加，勿空洞勾选。

---

## 2026-07-17 · 0.6.2 · 侧栏开合持久化

### DEFINE/PLAN

- `docs/product/changes/2026-07-17-sidebar-persist-DEFINE-PLAN.md`

### 命令

- [x] `verify.ps1 -StopRunning` — **26 passed / 1 ignored** + release
- [x] `pack.ps1` — `dist/蜜桃成熟-v0.6.2-win64.zip`（exe ~7.0 MB / zip ~3.7 MB）

### 手测

- [ ] 收起侧栏 → 退出 → 再开仍收起
- [ ] 展开侧栏 → 退出 → 再开仍展开
- [x] 单测：旧 config 无字段默认展开；false 可 roundtrip

### REVIEW

- PLAN P2-4 → done；版本 **0.6.2**  
- 产物：`dist/蜜桃成熟-v0.6.2-win64/`

---

## 2026-07-17 · 0.6.1 · P2-3 手测模板落地

### DEFINE/PLAN

- `docs/product/changes/2026-07-17-verify-template-DEFINE-PLAN.md`
- 基线清单：`docs/product/VERIFY-checklist.md`
- 脚本：`scripts/verify.ps1` · `scripts/new-verify-entry.ps1`

### 命令

- [x] `cargo test` — 25 passed / 1 ignored（via `scripts/verify.ps1 -StopRunning`）
- [x] `cargo build --release` — Finished（exe ~7.0 MB；脚本曾因路径编码误报 missing，已加固）

### 手测

发版时按 `VERIFY-checklist.md` 全表勾选；本条目仅验证流程工具可用。

- [x] checklist 文件存在
- [x] new-verify-entry 可写入（编码已修）
- [x] verify.ps1 可跑 test + release

### REVIEW

- PLAN P2-3 → done  
- 遗留：Windows PowerShell 5.1 写中文日志时脚本须 UTF-8 BOM  

---

## 2026-07-17 · 0.6.1 工作台壳层 + 置顶 + 多屏 + 图标

### DEFINE/PLAN

- `docs/product/changes/2026-07-17-workbench-shell-DEFINE-PLAN.md`
- `docs/product/changes/2026-07-17-multi-monitor-DEFINE-PLAN.md`

### 命令

- [x] `cargo test` — **25 passed / 1 ignored**（含 `list_monitors` / `resolve_work_area`）
- [ ] `cargo build --release` — 编译通过但链接时 **exe 被占用**（拒绝访问）；请关闭运行中的「蜜桃成熟」后执行 `cargo build --release`

### 手测清单

**工作台 / 置顶**

- [ ] 启动横版、窗口居中
- [ ] 侧栏图标隐藏/显示；隐藏后预览几乎全宽
- [ ] 顶栏调节本轮数量；侧栏无重复数量
- [ ] 随机预览片单单行；悬停仅一个无后缀名
- [ ] 电影 Playing + 置顶：面板在 PotPlayer 之上；关置顶可被盖住；最小化正常

**多屏（有副屏时）**

- [ ] 设置 → 平铺工作区选副屏 → 开启播放 → 窗落在副屏
- [ ] 改回「系统主工作区」→ 落主屏
- [ ] 拔掉所选屏后开播不崩溃（回退）

**图标**

- [ ] exe / 任务栏 / 托盘为新图标

### REVIEW

- PLAN P2-wb、P2-2 → done  
- DEFINE 已写工作台壳层、平铺选屏  
- 遗留：P2-3 手测模板；侧栏开合不持久化；显示器 index 热插拔可能漂移  

---

## 2026-07-17 · 0.6.0 发版 + P2-1 黑名单

### 命令

- [x] cargo test
- [x] cargo build --release
- [x] pack dist/蜜桃成熟-v0.6.0-win64.zip（脚本/手工）

### 手测建议

- [ ] 拉黑后随机预览不再出现该片
- [ ] 设置中移出黑名单后可再被抽到
- [ ] 单实例二次启动
- [ ] 多开平铺 + 重新平铺
- [ ] 索引缓存二次启动

### REVIEW

- 版本 0.6.0；P2-1 done  

---

## 2026-07-17 · P1-4 索引缓存 + 取消

### DEFINE/PLAN

- `docs/product/changes/2026-07-17-index-cache-cancel-DEFINE-PLAN.md`

### 命令

- [x] cargo test — 20 passed / 1 ignored（含 index_cache 单测）
- [x] cargo build --release — Finished

### 手测

- [ ] 大库第二次启动应明显快（读 index_cache）
- [ ] 索引中点「取消索引」
- [ ] 重新扫描强制全量并刷新缓存

### REVIEW

- PLAN P1-4 → done  

---

## 2026-07-17 · P1-3 拆分 app/mod.rs

### DEFINE

- 问题：`mod.rs` 过大难维护  
- 范围：拆 widgets / media_view / settings，无行为变更  
- 验收：`cargo test` + `cargo build --release` 通过  

### VERIFY

- [x] cargo test — 19 passed / 1 ignored  
- [x] cargo build --release — Finished  
- 行数约：mod 1106 / widgets 480 / settings 381 / media_view 291  

### REVIEW

- PLAN P1-3 → done；下一项 P1-4 索引缓存  

---

## 2026-07-17 · P1-2 单实例加固

### DEFINE/PLAN

- `docs/product/changes/2026-07-17-single-instance-DEFINE-PLAN.md`
- 跳过图片墙（用户：图片不需要）

### 命令

- [x] `cargo test` — 19 passed / 1 ignored
- [x] `cargo build --release` — Finished

### 手测

- [ ] 运行中再双击 exe → 无第二进程，原窗口前置
- [ ] 最小化到托盘后再双击 → 窗口恢复
- [ ] 成功前置时无弹「已在运行」对话框（找不到窗才提示）

### REVIEW

- Mutex + 命名 Event + `force_show_main_window_result`（AttachThreadInput 级前置）
- PLAN P1-2 → done；P1-1 → cancelled

---

## 2026-07-17 · 流程固化 + 基线

### 命令

- [x] `cargo test` — 此前 0.5.x 改动周期内 19 passed / 1 ignored（以当时终端为准）
- [x] `cargo build --release` — 成功产出 `蜜桃成熟.exe`

### 手测（基线，发版前应重跑）

- [ ] 电影：预览 → 开 6～10 部 → 网格稳定、无明显旧尺寸跳出
- [ ] 电影：重新平铺
- [ ] 电影播放中 → 图片 → 后台提示 → 关掉电影 / 切回关闭
- [ ] 图片：幻灯 / 墙 / Esc
- [ ] 设置：路径同一行序号；增删目录

### REVIEW

- 已建立 DEFINE/PLAN/流程文档
- 平铺问题已从「延后重铺」升级为「藏窗 + 几何守护」（见提交 d15a70e）
