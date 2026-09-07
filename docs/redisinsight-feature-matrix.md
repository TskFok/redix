# RedisInsight 非 Cloud 功能对比（2026-09-07）

参照本机 `/Users/ushopal/workspace/myself/RedisInsight` 源码，目标仓库未修改。
本轮基线：Redix `67318ae`，只读参考 RedisInsight `48ee19fab`；改动保留在当前 `main` 工作区，未提交或推送。
“基础支持”不表示与 RedisInsight 完全等价；仅已有源码和测试验证的能力可以标为支持。
表中参考源码路径均相对于目标仓库的 `redisinsight/` 目录。

| 功能 | Redix 原有状态 | 本轮变更与剩余差异 | 参考源码（RedisInsight） |
|---|---|---|---|
| Standalone、TLS、自定义 CA/mTLS、连接导入导出 | 已支持 | 保留凭据安全存储与无敏感信息导出 | `api/src/modules/database`、`database-import`、`certificate` |
| Sentinel | 基础支持 | 种子回退、主节点发现、独立认证、重连刷新，并支持 SSH+TLS；不宣称无缝故障迁移 | `api/src/modules/redis-sentinel` |
| SSH | 基础支持 | `ssh2` 严格主机校验，支持 Agent/Password/PrivateKey；TCP/握手/远端认证共享 6 秒预算，Agent 最多顺序尝试 32 个身份。本地 Agent IPC 仍可能不可中断并占用 worker/permit/CLI 锁；Standalone/Sentinel 可组合 TLS，Cluster+SSH 禁用。当前只在 macOS 自动化验证 | `api/src/modules/ssh` |
| Cluster / 节点路由 / 节点观察 | 本批新增 | DB 0 普通命令 slot 路由、跨 primary UTF-8 键名完整 SCAN、16384 slot 拓扑摘要和 primary-only analysis 已支持；二进制键名导致对应节点可重试失败并保留游标；typed SlowLog/PubSub/Profiler 禁用，节点现从已有INFO提取version/mode/totalkeys/maxmemory，缺失/异常不视作零 | `api/src/modules/cluster-monitor`、`ui/src/pages/redis-cluster` |
| 普通键浏览、新增/改名/删除/TTL、导入导出 | 基础支持 | 保留集合分页/增量编辑；新增选中键后台删除、进度、取消和结果未知计数（最多10000键、4并发、20条会话任务），Cluster使用按槽位选定的直连主节点且不重发；按模式全库任务、后台导入/导出及重启恢复仍有差异 | `api/src/modules/browser`、`bulk-actions` |
| 键树 | 缺失 | 支持可配置分隔符（默认 `:`，最多16字符）与平铺切换；列表2/5/10/30秒自动刷新默认关闭，选择键/查看详情/编辑及隐藏窗口时暂停 | `ui/src/pages/browser/components/key-tree` |
| JSON 根文档与路径编辑 | 已支持 API，缺树 | 本轮增加对象/数组折叠、节点定位、特殊属性安全路径及渲染上限 | `api/src/modules/browser/rejson-rl` |
| String 值多格式解码 | 基础文本 | 新增原始字节读取、UTF-8/ASCII/Hex/Binary/Base64/JSON编辑，Gzip/Zlib/Deflate压缩，MessagePack（含CSharp LZ4）、PHP serialized与无schema Protobuf只读；PHP对象/引用仅结构化展示，BOM与二进制键保留。输入4MiB，截断值不可保存。Java/Pickle、schema驱动Protobuf、专用向量/日期/Markdown视图及ZSTD/LZ4/Snappy/Brotli等压缩仍未实现；结构化Worker限时并限制输出，库内解析前分配仍有边界 | `ui/src/pages/browser/components/value-decoder` |
| Stream Group/消费者/Pending | 基础支持 | 保留消息范围分页/XADD/XDEL；新增XGROUP SETID、Pending范围/消费者分页、XCLAIM IDLE/TIME/RETRYCOUNT/FORCE/JUSTID。按目标源码核对，未发现Browser XREADGROUP/XAUTOCLAIM专用流程，撤销旧矩阵把它们列为目标差距的判断 | `api/src/modules/browser/stream` |
| RedisSearch 索引与查询 | 已支持受限 NOCONTENT | 保留字段结果/RESP3/受限聚合；修复VECTOR创建，增加FLAT/HNSW完整配置与同连接版本校验；新增Text/Tag/Numeric/Geo可视过滤、预览和显式回填。新增typed二进制PARAMS/KNN专用UI、服务端schema验证、FLOAT32/FLOAT64、K≤200、距离结果；创建字段可显式设置AS查询别名用于JSONPath。更高级向量类型与完整过滤语法仍有差异 | `api/src/modules/browser/redisearch`、`ui/src/pages/vector-search` |
| Array / Vector Set | 基础支持 | 保留既有 bounded 编辑、搜索、向量下载；并非所有高级命令专用 UI | `api/src/modules/browser/array`、`vector-set` |
| Workbench | 基础执行/历史/格式 | 本轮增加模块命令帮助、光标行补全、注释、结果树/表、历史清理，以及绑定已执行命令的TimeSeries/Geo本地图表（2000点/20序列） | `api/src/modules/workbench`、`commands` |
| 独立 CLI 会话 | 基础支持 | Standalone/Sentinel 使用专用持久 socket；Cluster 只支持普通路由命令，事务、WATCH、SELECT 及其他 socket 状态命令与 Workbench single/batch 同样在发送前拒绝 | `api/src/modules/cli`、`ui/src/components/cli` |
| Slow Log / PubSub / Profiler | Standalone/Sentinel 基础支持 | Cluster typed Slow Log 四端点、PubSub 与 Profiler 禁用；原生命令仅由驱动决定路由范围，不承诺全拓扑语义 | `api/src/modules/slow-log`、`pub-sub`、`profiler` |
| 实例概览 / 数据库分析 | 基础支持 | 保留primary汇总和失败节点；新增基于Top Keys阈值的可解释本地建议、相同参数/节点范围历史键数和内存趋势。新增可取消/总超时/跨页面恢复的后台分析，绑定原连接代次；Cluster余数配额按节点ID固定。实例概览新增120次内存/操作数/客户端趋势与默认关闭自动刷新。完整推荐策略/状态与长期监控历史仍有差异 | `api/src/modules/database-analysis`、`database-recommendation` |
| Query Library / 设置 | 基础支持 | 新增连接key/value标签、搜索/无标签过滤，以及版本化查询包JSON导入导出、原子容量验证和并发保护；新增Ctrl/Cmd+K操作面板、常用导航/聚焦快捷键、键盘与IME/焦点保护；按索引查询分区、通知和自动更新仍有差异 | `api/src/modules/query-library`、`tag`、`settings` |
| 模块结果插件可视化 | 缺失 | 新增白名单TimeSeries/Geo可视化与有界数据表，结果形状异常时保留原始结果并提示；不引入插件运行时，完整插件能力仍有差异 | `ui/src/packages`、`api/src/modules/plugin` |
| Redis Cloud | 明确排除 | 不实现登录、账户、云资源发现与云 SDK | `api/src/modules/cloud` |

永久排除 Redis Cloud、Azure Managed Redis、RDI、AI/Copilot、Telemetry/Analytics 与远程插件；其他本地差异仍是后续工作。本轮在既有连接拓扑基础上继续补齐Browser、Search及本地产品的上述工作流；表中明确列出的剩余差异仍未完成，不能称为全量等价。

## 上一连接与拓扑批次验证（历史记录）

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

## 本轮验证与审查

本轮最终计数见 `docs/local-parity-2026-09-07.md`。新增行为先验证失败再实现；交叉审查发现并修复了Cluster删除重试、任务轮询停止、标签原型属性、只读ACL、LZ4无效回溯、Protobuf整数精度、JSON/Search时序、PHP BOM/二进制键、图表极值/长字段及Cluster配额顺序问题。未运行的环境不计为通过。
