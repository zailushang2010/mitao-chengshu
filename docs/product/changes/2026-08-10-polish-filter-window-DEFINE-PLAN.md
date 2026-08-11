# CHANGE: 假入口清理 + 窗口几何记忆

## DEFINE

### 问题
1. 操作条「筛选 / 排序」点了只 toast「即将推出」，像半成品。  
2. 窗口大小与位置每次重启回到默认居中，日用摩擦大。

### 范围内
- 去掉假「筛选 / 排序」chip  
- 本轮搜索（顶栏）为唯一筛选入口；有字时显示清除；无预览时弱化  
- 记忆窗口 **inner 尺寸 + outer 位置** 到 `config.json`  
- 重启恢复；位置不在任何显示器工作区附近则回退默认尺寸并居中  
- 拖移/缩放防抖写入（约 0.6s）

### 范围外
- 全库筛选 / 复杂排序  
- 记忆最大化/全屏状态  
- 按显示器设备名绑定（另项）

### 验收
- [ ] 无「即将推出」筛选/排序入口  
- [ ] 本轮搜索可筛卡片；清除可一键清空  
- [ ] 改窗口大小/位置 → 重启后仍接近上次  
- [ ] 拔掉副屏后位置失效 → 不飞出屏幕（回退居中）  
- [ ] cargo test / release 通过  

## PLAN

| # | 任务 | 文件 |
|---|------|------|
| 1 | `WindowGeometry` + load/save/validate | config.rs, tiler 可选 |
| 2 | main 启动应用几何；无记忆则 center | main.rs |
| 3 | update 防抖写回；need_center 仅无记忆时 | app/mod.rs, session |
| 4 | 删假 chip；搜索清除 | widgets + mod |
| 5 | DEFINE/PLAN/VERIFY 摘要 |

### 风险
- 多屏坐标系与 DPI：存 outer 点坐标 + inner 点尺寸；校验用 work area  
- 启动帧 center 与记忆冲突：有记忆时 `need_center=false`
