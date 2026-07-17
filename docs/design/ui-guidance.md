# 界面设计指导（蜜桃成熟）

## 来源

- **Emil Kowalski Skills**：https://github.com/emilkowalski/skills  
  动效决策、组件细节、克制原则（Vercel / Linear 一线经验）
- **本项目适配 skill**：`.agents/skills/ui-emil-design/SKILL.md`  
  把 Web/CSS 规则翻译成 egui + 纸感杂志风

## 怎么用

| 场景 | 做法 |
|------|------|
| 改按钮 / toast / 面板 | 先读 `ui-emil-design` skill |
| 想「加点动画」 | 先过频率闸门；过不了就不做 |
| 审查一版 UI | 用 Before / After / Why 表 |
| 需要精确动效名词 | 上游 `animation-vocabulary` |

## 和本产品的契合点

Emil 的核心不是「多动画」，而是：

1. **看不见的细节叠加**才形成质感  
2. **按压反馈、即时响应**优先于装饰  
3. **高频操作零动画**（我们的模式切换、±数量）  
4. **偶尔出现**的状态（toast、设置、幻灯）才值得短过渡  

这与「纸感杂志 · 安静选片」一致：安静、利落，不花哨。

## 当前界面机会（审计摘要）

| # | 位置 | 现状 | 目的 | 建议 |
|---|------|------|------|------|
| 1 | 主/次按钮、icon | 仅 hover 变色 | Feedback | pointer-down 内缩/加深 100–160ms |
| 2 | Toast | 硬显硬消 | 防跳跃 | 150–200ms fade；替换可打断 |
| 3 | 幻灯切图 | 硬切纹理 | 防跳跃 | 短 crossfade opacity |
| 4 | 设置窗 | 瞬开 | 防跳跃 | ≤200ms fade（可选） |

**刻意不做**：模式 chip 弹跳、数量 stepper 动画、预览全表长 stagger。

## 实现备注（egui）

- 动画状态放在 `SuijiApp`：`press_scale: f32`、`toast_alpha`、`slide_fade`  
- 每帧 `dt` 插值；`ease_out = 1 - (1-t)^3` 足够  
- 只改绘制参数，避免 `allocate` 尺寸每帧变化导致布局抖动  
