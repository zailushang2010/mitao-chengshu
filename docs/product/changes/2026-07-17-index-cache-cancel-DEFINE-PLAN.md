# CHANGE: 索引缓存 + 可取消

## DEFINE

| 字段 | 内容 |
|------|------|
| 问题 | 大库每次启动/切换都全量扫盘慢；索引中无法中止 |
| 范围内 | 按根目录磁盘缓存；手动「重新扫描」强制刷新；索引中可取消 |
| 范围外 | 文件级实时监控（watch）、云端索引 |
| 验收 | ① 二次启动同目录明显更快（走缓存）② 重新扫描强制全量 ③ 索引中点取消停止并提示 ④ test/release 通过 |

## PLAN

1. `index_cache`：每根目录 + 扩展名 → JSON 缓存（含 root mtime 签名）  
2. `Library::scan_*` 支持 `is_cancelled`  
3. `begin_scan(force)`；`rescan` force=true；启动 force=false  
4. `cancel_scan` + UI「取消索引」  
5. 增删目录时删对应缓存  

## BUILD

- `src/index_cache.rs` 每根目录 JSON 缓存  
- 扫描可取消；UI「取消索引」  
- `rescan` / 增删路径 force=true  

## VERIFY

- [x] cargo test / release  
- [ ] 手测大库二次启动、取消、重新扫描  

## REVIEW

- DEFINE 验收与已知限制已更新；PLAN P1-4 done  
