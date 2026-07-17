# 贡献指南

感谢你愿意改进 **蜜桃成熟**。

## 开发流程（本仓库强制）

```text
DEFINE → PLAN → BUILD → VERIFY → REVIEW
```

详见：

- [`AGENTS.md`](./AGENTS.md)  
- [`docs/process/lifecycle.md`](./docs/process/lifecycle.md)  
- 产品范围：[`docs/product/DEFINE.md`](./docs/product/DEFINE.md)  

较大改动请先开 Issue 说明问题与验收标准，或附上简短 DEFINE/PLAN，避免范围蔓延。

## 环境

- Windows  
- Rust stable（`rustup`）  
- 可选：PotPlayer、ffmpeg  

## 常用命令

```powershell
cargo test
cargo build --release
powershell -File scripts/verify.ps1 -StopRunning
powershell -File scripts/pack.ps1
```

## 提交建议

- 说明用中文完整句亦可；type 可用 `feat` / `fix` / `docs` 等  
- 一次提交聚焦一件事  
- 涉及播放/平铺：在 PR 中写清手测步骤  
- **不要**提交 `config.json`、历史、黑名单、缩略图缓存、个人路径  

## PR 检查清单

- [ ] `cargo test` 通过  
- [ ] 不引入未说明的范围外行为  
- [ ] 用户可见文案通顺  
- [ ] 若改产品行为：同步 `docs/product/DEFINE.md` 或 changes  

## 行为准则

请保持友善、对事不对人。不接受骚扰或恶意破坏。维护者有权关闭不当 Issue/PR。
