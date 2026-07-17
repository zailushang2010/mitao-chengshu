# CHANGE: P2-3 VERIFY 手测清单模板

## DEFINE

### 问题

发版/合入时 VERIFY 手测靠记忆，日志格式不一，易漏项；与 DEFINE §7 验收不同步。

### 范围内

- 固定「发版 VERIFY 清单」模板（对齐 DEFINE §7 + 常见回归）
- 一键脚本：向 `VERIFY-log.md` 追加一条带版本/日期的空清单
- 一键脚本：跑 `cargo test` + `cargo build --release` 并提示手测
- 生命周期文档指向模板与脚本

### 范围外

- GUI 自动化点选（不在本周期）
- CI 云端 Windows  runner（无强制要求）

### 验收

- [ ] 存在可复制的手测模板文件
- [ ] `scripts/new-verify-entry.ps1` 能在 VERIFY-log 顶部生成条目
- [ ] `scripts/verify.ps1` 能跑测试与 release（exe 未占用时）
- [ ] PLAN P2-3 → done

## PLAN

| # | 任务 |
|---|------|
| 1 | `docs/product/VERIFY-checklist.md` 基线清单 |
| 2 | `scripts/new-verify-entry.ps1` |
| 3 | `scripts/verify.ps1` |
| 4 | 更新 lifecycle / PLAN / VERIFY-log |
| 5 | 本机跑一遍脚本 |

## BUILD

- `docs/product/VERIFY-checklist.md`
- `scripts/new-verify-entry.ps1`
- `scripts/verify.ps1`
- `lifecycle.md` / `PLAN.md` 挂钩

## VERIFY / REVIEW

- 脚本试跑 + cargo test；P2-3 done
