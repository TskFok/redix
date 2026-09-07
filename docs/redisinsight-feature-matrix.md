# RedisInsight 非 Cloud 功能对比（2026-09-01）

参照本机 `/Users/ushopal/workspace/myself/RedisInsight` 源码，目标仓库未修改。
连接与拓扑批次产品基线：Redix `1ae35dd`（Task10 执行起点 `30b1ec9`），只读参考 RedisInsight `48ee19fab`；本轮改动保留在当前 `main` 工作区。
“基础支持”不表示与 RedisInsight 完全等价；仅已有源码和测试验证的能力可以标为支持。
表中参考源码路径均相对于目标仓库的 `redisinsight/` 目录。

| 功能 | Redix 原有状态 | 本轮变更与剩余差异 | 参考源码（RedisInsight） |
|---|---|---|---|
| Standalone、TLS、自定义 CA/mTLS、连接导入导出 | 已支持 | 保留凭据安全存储与无敏感信息导出 | `api/src/modules/database`、`database-import`、`certificate` |
| Sentinel | 基础支持 | 种子回退、主节点发现、独立认证、重连刷新，并支持 SSH+TLS；不宣称无缝故障迁移 | `api/src/modules/redis-sentinel` |
| SSH | 基础支持 | `ssh2` 严格主机校验，支持 Agent/Password/PrivateKey；TCP/握手/远端认证共享 6 秒预算，Agent 最多顺序尝试 32 个身份。本地 Agent IPC 仍可能不可中断并占用 worker/permit/CLI 锁；Standalone/Sentinel 可组合 TLS，Cluster+SSH 禁用。当前只在 macOS 自动化验证 | `api/src/modules/ssh` |
| Cluster / 节点路由 / 节点观察 | 本批新增 | DB 0 普通命令 slot 路由、跨 primary UTF-8 键名完整 SCAN、16384 slot 拓扑摘要和 primary-only analysis 已支持；二进制键名导致对应节点可重试失败并保留游标；typed SlowLog/PubSub/Profiler 禁用，节点 DTO 尚无 version/mode/totalkeys 等目标额外指标 | `api/src/modules/cluster-monitor`、`ui/src/pages/redis-cluster` |
| 普通键浏览、新增/改名/删除/TTL、导入导出 | 基础支持 | 本轮增加 Hash/List/Set/ZSet 字段级分页与原位编辑；批量后台任务仍有差异 | `api/src/modules/browser`、`bulk-actions` |
| 键树 | 缺失 | 本轮增加 `:` 前缀树与平铺切换，仅处理已扫描结果 | `ui/src/pages/browser/components/key-tree` |
| JSON 根文档与路径编辑 | 已支持 API，缺树 | 本轮增加对象/数组折叠、节点定位、特殊属性安全路径及渲染上限 | `api/src/modules/browser/rejson-rl` |
| String 值多格式解码 | 基础文本 | 尚无 MessagePack、Protobuf、压缩等完整解码器体系 | `ui/src/pages/browser/components/value-decoder` |
| Stream Group/消费者/Pending | 基础支持 | 本轮增加消息范围分页、XADD/XDEL 原位操作和显式 XCLAIM；实时消费、XAUTOCLAIM 及高级 Claim 选项尚有差异 | `api/src/modules/browser/stream` |
| RedisSearch 索引与查询 | 已支持受限 NOCONTENT | 本轮增加字段内容结果、RESP3 兼容及受限 FT.AGGREGATE；可视查询构建器/向量索引高级流程仍有差异 | `api/src/modules/browser/redisearch`、`ui/src/pages/vector-search` |
| Array / Vector Set | 基础支持 | 保留既有 bounded 编辑、搜索、向量下载；并非所有高级命令专用 UI | `api/src/modules/browser/array`、`vector-set` |
| Workbench | 基础执行/历史/格式 | 本轮增加模块命令帮助、光标行补全、注释、结果树/表及历史清理 | `api/src/modules/workbench`、`commands` |
| 独立 CLI 会话 | 基础支持 | Standalone/Sentinel 使用专用持久 socket；Cluster 只支持普通路由命令，事务、WATCH、SELECT 及其他 socket 状态命令与 Workbench single/batch 同样在发送前拒绝 | `api/src/modules/cli`、`ui/src/components/cli` |
| Slow Log / PubSub / Profiler | Standalone/Sentinel 基础支持 | Cluster typed Slow Log 四端点、PubSub 与 Profiler 禁用；原生命令仅由驱动决定路由范围，不承诺全拓扑语义 | `api/src/modules/slow-log`、`pub-sub`、`profiler` |
| 实例概览 / 数据库分析 | 基础支持 | Cluster analysis 汇总 primary 并报告失败节点；仍缺完整时间序列图、后台分析和推荐引擎 | `api/src/modules/database-analysis`、`database-recommendation` |
| Query Library / 设置 | 基础支持 | 未全面对齐查询包、连接标签、快捷键、自动更新等产品细节 | `api/src/modules/query-library`、`tag`、`settings` |
| 模块结果插件可视化 | 缺失 | 本轮不引入插件运行时，模块命令可在 Workbench 执行 | `ui/src/packages`、`api/src/modules/plugin` |
| Redis Cloud | 明确排除 | 不实现登录、账户、云资源发现与云 SDK | `api/src/modules/cloud` |

永久排除 Redis Cloud、Azure Managed Redis、RDI、AI/Copilot、Telemetry/Analytics 与远程插件；其他本地差异仍是后续工作。当前只收口六批路线中的连接与拓扑第一批，Browser 解码器、后台任务等后续批次尚未完成。

## 本轮验证

| 验证 | 结果 |
|---|---|
| 前端回归 | 35 文件、297 项通过；两类既有 jsdom/重复 saved key 警告不计失败 |
| Rust 常规回归 | 374 项通过、29 项默认 ignored；ignored 未计为实际通过，保留 5 个既有 Array dead-code 警告和测试 helper 警告 |
| 隔离普通 Redis | 7 个实际流程通过；另 3 个 Redis Stack 流程因未配置环境而 early-skip |
| 隔离 Cluster | launcher 安全单测 6 项、真实三主节点集成 2 项通过；覆盖跨 primary UTF-8 键名完整 SCAN、拓扑、analysis、CROSSSLOT 与无副作用门控 |
| SSH 路径兼容 focused | connections commands 32 项、ssh 模块 26 项、Sentinel 集成 8 项通过（另 2 项默认 ignored）；没有运行真实 sshd |
| 前端生产构建 | TypeScript / Vite 94 模块通过 |
| macOS 原生 release 构建 | `CARGO_NET_OFFLINE=true npm run tauri:build -- --no-bundle` 通过（1 分 16 秒）；未打包、签名、安装或发布 |
| 范围与格式 | non-cloud、范围扫描正反例、cargo fmt --check、git diff --check 通过 |

未运行 Redis Stack、真实 sshd、真实 TLS/mTLS 组合、Windows/Linux 原生验证或完整 Tauri 桌面交互端到端；`.github/workflows/cross-platform.yml` 只是 CI 配置，本轮未在 GitHub 上执行，不能据此宣称三平台通过。响应限制主要约束解析、IPC 与展示，不能替代服务端资源限制或 Redis 客户端底层协议读取的内存限制。
