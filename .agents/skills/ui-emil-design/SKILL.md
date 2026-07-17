---
name: ui-emil-design
description: >
  蜜桃成熟 UI 品味规范。基于 Emil Kowalski design engineering（emilkowalski/skills）
  与本项目纸感杂志风 egui 桌面端适配。做界面、动效、按钮反馈、toast、模式切换时加载本 skill。
---

# 蜜桃成熟 · Design Engineering（Emil 适配）

上游：https://github.com/emilkowalski/skills  
核心：`emil-design-eng` · `apple-design` · `find-animation-opportunities` · `animation-vocabulary`

本产品是 **Windows 原生 egui 应用**，不是 Web。CSS / Framer Motion 规则要**翻译**成：
- egui `Response` 状态（hovered / clicked / is_pointer_button_down）
- 自绘 `painter` + 短时动画状态（`t: f32` 插值）
- 只动 **颜色、不透明度、轻微 scale 感（矩形尺寸）**；避免昂贵每帧布局抖动

## 产品个性（约束一切动效）

| 维度 | 选择 |
|------|------|
| 气质 | 纸感杂志 · 克制 · 安静 |
| 使用频率 | 中等（选片/切模式/预览），不是 Raycast 级极高频 |
| 动效预算 | **少而准**；默认「可以不动画」 |
| 禁止 | 装饰性悬停缩放、键盘操作动画、列表每次刷新全量 stagger |

## 决策闸门（动之前必答）

1. **频率**  
   - 日百次（± 数量、模式连点）→ **禁止动画**  
   - 日数十次（hover）→ 无动画或极短色变  
   - 偶尔（toast、设置面板、幻灯进出）→ 标准短反馈  
   - 罕见（首次空态）→ 可轻微 delight  

2. **目的**（必须能命名其一）  
   Feedback · Spatial consistency · State indication · 防跳跃 · Explanation · Delight(仅罕见)

3. **时长**（egui 建议）  
   - 按压缩放感：100–160ms  
   - Toast 进出：150–250ms  
   - 设置/叠层：200–300ms 内  
   - **UI 动效 ≤ 300ms**

4. **缓动**  
   - 进入 / 响应用户 → ease-out 感（先快后慢）  
   - 屏上移动 → ease-in-out  
   - **禁止 ease-in 作 UI 响应**（会显钝）

## 组件铁律（对本仓库）

### 按钮必须有按压反馈
- Primary / Secondary / mini / icon：`clicked` 帧或 pointer down 时  
  背景略深 **或** 内容区视觉 scale ≈ **0.97**（用 rect 内缩 1–2px 模拟，勿真改布局导致跳动）
- 禁用态只变色，不假装可点

### Toast
- 不要用缺字符号（✓ 已踩坑 → 绿点/绘制）
- 进出同一边（自上滑入 / 自上滑出或淡出）
- 可打断：新 toast 替换旧的时从当前 alpha 过渡，不要硬切
- 时长显示 ~2s；进出动画 150–200ms ease-out

### 模式切换（电影 / 图片）
- **禁止**整页动画；索引后台已做，UI 只换内容
- Chip 选中态：即时填色即可，勿 scale 弹跳

### 预览网格
- 新预览出现：可选 **极短 stagger**（30–50ms × 最多 6 项）仅 opacity；可交互期间绝不阻塞
- 剔除一项：该行 fade 出，其余上移（有成本则先做 fade）

### 设置面板
- 打开：opacity 0→1 + 轻微自中心 scale 0.97→1（≤200ms）
- 关闭更快于打开（asymmetric）

### 幻灯 / 平铺墙
- 切图：优先 **crossfade**（opacity），勿硬切
- Esc 结束：立即响应，可无退场动画

## 与纸感杂志风一致的视觉

已有 token（`theme.rs`）：`BG` / `INK` / `MUTED` / `LINE` …  
- 阴影用**半透明**，少用死黑描边堆叠  
- 大标题 tracking 可略紧；正文 tracking 近 0  
- 层次靠 **字重 + 间距 + 对比**，不靠彩虹色

## Review 输出格式（改 UI 时强制）

用表格，不要散装 Before/After：

| Before | After | Why |
| --- | --- | --- |
| 主按钮无按下态 | pointer-down 时 rect 内缩/底色加深 100–160ms | Press feedback |

## 明确不要做

| 候选 | 拒绝原因 |
|------|----------|
| 电影↔图片 chip 弹跳 | 高频切换；已优化索引，再动画只会显慢 |
| 本轮数量 ± 的数字滚动 | 高频；tabular 固定宽即可 |
| 全量预览卡进场 300ms stagger | 防拖沓；最多短 opacity |
| 设置里每个 toggle 弹簧 | 无必要；即时状态足够 |

## 推荐落地顺序（对本 app）

1. 所有可点控件 **press feedback**（最高杠杆）  
2. Toast 进出一致 + 可替换  
3. 幻灯切图 **crossfade**  
4. 设置面板短 fade  
5. 预览剔除短 fade（可选）

## 上游原文索引

- 主 skill：`emil-design-eng`  
- 克制找机会：`find-animation-opportunities`  
- 词表：`animation-vocabulary`  
- Apple 流体：`apple-design`  

安装上游全集（本机 agent 环境）：  
`npx skills@latest add emilkowalski/skills`
