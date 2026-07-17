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
| P1-1 | todo | 图片墙缩略图化，点开再原图 | 大图墙不爆内存；墙面可滚/可点 |
| P1-2 | todo | 单实例启动（二次启动激活已有窗口） | 双开配置不打架 |
| P1-3 | todo | 拆分 `app/mod.rs`（settings/slideshow/buttons） | 编译通过；行为无回归 |
| P1-4 | todo | 索引可取消 + 可选根目录缓存 | 大库二次启动更快 |
| P2-1 | todo | 黑名单/不再抽到 | 剔除可持久排除 |
| P2-2 | todo | 多显示器选择工作区 | 平铺落在指定屏 |
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
