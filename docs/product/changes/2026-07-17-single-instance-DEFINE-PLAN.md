# CHANGE: 单实例启动加固

## DEFINE

| 字段 | 内容 |
|------|------|
| 问题 | 重复双击可能开多个进程，或第二次启动找不到已隐藏/托盘中的窗口 |
| 范围内 | 全局仅一个进程；第二次启动激活已有窗口（含托盘隐藏） |
| 范围外 | 多开调试专用开关（debug 可另议） |
| 验收 | ① 已运行时再开 exe → 不出现第二个进程 ② 主窗口或托盘隐藏时再开 → 窗口被带到前台 ③ 配置不被双开踩乱 |

## PLAN

1. 保留命名 Mutex 判重  
2. 命名 Event：第二实例 `SetEvent`，第一实例 `update` 中检测并 `force_show`  
3. 第二实例用与托盘相同的 `raise`（含 AttachThreadInput）  
4. 成功前置则**不**弹 MessageBox；失败再提示  
5. PLAN：跳过图片墙缩略图（用户明确「图片不需要」）

## BUILD

- 命名 Event `SHOW_EVENT_NAME`；第二实例 `SetEvent` + Win32 强力前置
- 第一实例 waiter → `AtomicBool` → `SuijiApp::show_window`
- 前置成功不弹 MessageBox

## VERIFY

- [x] cargo test / cargo build --release
- [ ] 手测双击 / 托盘（见 VERIFY-log）

## REVIEW

- DEFINE 已写单实例；PLAN P1-2 done，P1-1 cancelled  
