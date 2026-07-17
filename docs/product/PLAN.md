# 蜜桃成熟 · PLAN（滚动计划）

> 依据：`docs/product/DEFINE.md`  
> 规则：只把 **已 DEFINE** 的条目拆进 BUILD；完成后勾 VERIFY/REVIEW  

状态：`todo` | `doing` | `done` | `blocked`

---

## 当前焦点

| ID | 状态 | 项 | 验收要点 |
|----|------|-----|----------|
| P0-done | done | 音量 UI 移除、索引进度、PID 探活 | DEFINE §7 相关 |
| P0-done | done | 电影/图片并存（park） | DEFINE §4.1 并存 |
| P0-done | done | 平铺根因：藏窗入格 + 全程几何守护 | DEFINE §7 电影网格 |
| P1-1 | cancelled | 图片墙缩略图化 | 用户明确暂不需要图片向增强 |
| P1-2 | done | 单实例启动加固（Event 唤醒 + 强力前置） | 二次启动不新开进程；托盘隐藏可拉回 |
| P1-3 | done | 拆分 `app/mod.rs` → widgets / media_view / settings | test+release 通过；行为等价 |
| P1-4 | done | 索引可取消 + 根目录磁盘缓存 | 二次启动走缓存；取消索引；重新扫描强制 |
| P2-1 | done | 黑名单/不再抽到 | 拉黑持久排除；设置可移出；0.6.0 |
| P2-wb | done | 工作台壳层 + 置顶修复 + 新图标 | 见 changes/2026-07-17-workbench-shell；0.6.1 |
| P2-2 | done | 多显示器选择工作区 | 设置下拉；resolve_work_area；0.6.1 |
| P2-3 | todo | VERIFY 手测清单自动化记录模板 | 每次发版有 log |

---

## 进行中变更（模板）

复制到 `docs/product/changes/` 使用：

```markdown
# CHANGE: 短标题
## DEFINE
- 问题：
- 范围内 / 外：
- 验收：
## PLAN
- 方案：
- 任务：
- 风险：
## BUILD
- 提交：
## VERIFY
- [ ] cargo test
- [ ] cargo build --release
- [ ] 手测：
## REVIEW
- 是否更新 DEFINE/PLAN：
- 遗留：
```

---

## 明确不做（本周期）

- 音量自动控制 PotPlayer
- 非 Windows 移植
- 云端功能

---

## 发版检查（REVIEW 用）

- [ ] DEFINE 已知限制已更新
- [ ] README 与行为一致
- [ ] 版本号 / `scripts/pack.ps1` 产物
- [ ] `cargo test` + release 有证据
