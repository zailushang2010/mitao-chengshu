# VERIFY 日志

按 `docs/process/lifecycle.md` 追加，勿空洞勾选。

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
