# CHANGE: 工作台侧栏开合持久化

## DEFINE

### 问题

侧栏每次启动默认打开，收起预览全屏的用法无法保留，和「主区给预览」冲突。

### 范围内

- `config.json` 记住操作栏开/关
- 启动时恢复上次状态（无动画跳变：直接到位）
- 顶栏侧栏按钮 / 预览区「操作栏」切换时立即写入配置

### 范围外

- 记住窗口大小/位置
- 侧栏宽度拖拽
- 显示器设备名匹配（另项）

### 验收

- [ ] 收起侧栏 → 退出 → 再开：仍收起
- [ ] 展开侧栏 → 退出 → 再开：仍展开
- [ ] 旧配置无字段时默认打开（兼容）
- [ ] cargo test + release / pack

## PLAN

1. `Config.workbench_sidebar_open` + default true  
2. `SessionHandle::set_workbench_sidebar_open`  
3. `SuijiApp::new` 读配置；toggle 时写回  
4. 版本 0.6.2；VERIFY + pack  

## BUILD

- `Config.workbench_sidebar_open`
- 切换侧栏时 `set_workbench_sidebar_open`；启动恢复且 `sidebar_vis` 直接到位

## VERIFY / REVIEW

见 `VERIFY-log.md` · 0.6.2；pack 已出 zip。
