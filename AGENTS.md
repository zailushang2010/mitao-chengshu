# 蜜桃成熟 · Agent 工作约定

本产品开发 **必须** 遵守生命周期：

```text
DEFINE → PLAN → BUILD → VERIFY → REVIEW
```

未完成 DEFINE / PLAN 不得直接大改；未 VERIFY 不得声称完成；未 REVIEW 不得当收尾。

细则见：`docs/process/lifecycle.md`  
当前产品定义：`docs/product/DEFINE.md`  
当前计划：`docs/product/PLAN.md`

## 阶段门槛（硬性）

| 阶段 | 产出 | 禁止 |
|------|------|------|
| **DEFINE** | 问题、用户、范围、非目标、验收标准写清 | 边做边改需求、无验收标准就开码 |
| **PLAN** | 任务拆分、优先级、风险、涉及文件 | 无计划直接大范围改架构 |
| **BUILD** | 按 PLAN 实现，小步提交 | 范围蔓延、顺手改无关模块 |
| **VERIFY** | 可复现的验证（test/build/手测清单）有证据 | 「应该没问题」式完成 |
| **REVIEW** | 对照 DEFINE 验收 + 风险/后续 | 跳过对照直接合入心态 |

## 本仓库习惯

- 语言：用户沟通与提交说明优先中文完整句
- 栈：Rust + egui + PotPlayer（Windows）
- UI 手感：`.agents/skills/ui-emil-design` + `docs/design/ui-guidance.md`
- 验证底线：`cargo test` + `cargo build --release`；涉及播放/平铺必须写清手测步骤
- 配置：不写死用户盘符；`library_paths` 权威；legacy 字段勿复活路径

## 变更最小记录（BUILD 起）

每次功能/修复在对话或 PR/提交中至少包含：

1. **对应 PLAN 条目**（或说明为何是 hotfix）
2. **VERIFY 结果**（命令输出结论 + 手测项）
3. **是否改动 DEFINE**（改了就要更新 `docs/product/DEFINE.md`）
