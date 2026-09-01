# RedisInsight 非 Cloud 功能对比（2026-09-01）

参照本机 `/Users/ushopal/workspace/myself/RedisInsight` 源码，目标仓库未修改。
本轮对比基线：Redix `882386d`，RedisInsight `48ee19fab`；本轮改动保留在当前 `main` 工作区。
“基础支持”不表示与 RedisInsight 完全等价；仅已有源码和测试验证的能力可以标为支持。
表中参考源码路径均相对于目标仓库的 `redisinsight/` 目录。

| 功能 | Redix 原有状态 | 本轮变更与剩余差异 | 参考源码（RedisInsight） |
|---|---|---|---|
| Standalone、TLS、自定义 CA/mTLS、连接导入导出 | 已支持 | 保留凭据安全存储与无敏感信息导出 | `api/src/modules/database`、`database-import`、`certificate` |
| Sentinel | 缺失 | 本轮实现种子回退、主节点发现、独立认证、重连刷新；不宣称无缝故障转移 | `api/src/modules/redis-sentinel` |
| SSH | 缺失 | 本轮实现 macOS/Linux OpenSSH 严格主机校验、认证后的转发确认与子进程清理；限 Standalone 非 TLS，不支持 Windows/密码认证/SSH+TLS/Sentinel | `api/src/modules/ssh` |
| Cluster / 节点路由 / 节点观察 | 缺失 | 尚未实现；需要独立的 slot 路由与跨节点 SCAN/观察语义 | `api/src/modules/cluster-monitor`、`ui/src/pages/redis-cluster` |
| 普通键浏览、新增/改名/删除/TTL、导入导出 | 基础支持 | 本轮增加 Hash/List/Set/ZSet 字段级分页与原位编辑；批量后台任务仍有差异 | `api/src/modules/browser`、`bulk-actions` |
| 键树 | 缺失 | 本轮增加 `:` 前缀树与平铺切换，仅处理已扫描结果 | `ui/src/pages/browser/components/key-tree` |
| JSON 根文档与路径编辑 | 已支持 API，缺树 | 本轮增加对象/数组折叠、节点定位、特殊属性安全路径及渲染上限 | `api/src/modules/browser/rejson-rl` |
| String 值多格式解码 | 基础文本 | 尚无 MessagePack、Protobuf、压缩等完整解码器体系 | `ui/src/pages/browser/components/value-decoder` |
| Stream Group/消费者/Pending | 基础支持 | 本轮增加消息范围分页、XADD/XDEL 原位操作和显式 XCLAIM；实时消费、XAUTOCLAIM 及高级 Claim 选项尚有差异 | `api/src/modules/browser/stream` |
| RedisSearch 索引与查询 | 已支持受限 NOCONTENT | 本轮增加字段内容结果、RESP3 兼容及受限 FT.AGGREGATE；可视查询构建器/向量索引高级流程仍有差异 | `api/src/modules/browser/redisearch`、`ui/src/pages/vector-search` |
| Array / Vector Set | 基础支持 | 保留既有 bounded 编辑、搜索、向量下载；并非所有高级命令专用 UI | `api/src/modules/browser/array`、`vector-set` |
| Workbench | 基础执行/历史/格式 | 本轮增加模块命令帮助、光标行补全、注释、结果树/表及历史清理 | `api/src/modules/workbench`、`commands` |
| 独立 CLI 会话 | 缺失 | 本轮增加持久 socket、跨轮事务/WATCH/SELECT，关闭和超时清理；记录只在内存，推送与协议切换受限 | `api/src/modules/cli`、`ui/src/components/cli` |
| Slow Log / PubSub / Profiler | 实时基础支持 | 本轮补用户显式文件导出与过滤/暂停展示；不自动持久化 | `api/src/modules/slow-log`、`pub-sub`、`profiler` |
| 实例概览 / 数据库分析 | 基础支持 | 本轮增加显式本机历史和相同参数观察值比较；仍缺完整时间序列图、后台分析和推荐引擎 | `api/src/modules/database-analysis`、`database-recommendation` |
| Query Library / 设置 | 基础支持 | 未全面对齐查询包、连接标签、快捷键、自动更新等产品细节 | `api/src/modules/query-library`、`tag`、`settings` |
| 模块结果插件可视化 | 缺失 | 本轮不引入插件运行时，模块命令可在 Workbench 执行 | `ui/src/packages`、`api/src/modules/plugin` |
| Redis Cloud | 明确排除 | 不实现登录、账户、云资源发现与云 SDK | `api/src/modules/cloud` |

旧 MVP 文档将很多尚未实现功能称为“排除项”。本轮差异表明确区分用户要求排除的 Redis Cloud 和仍需补齐的本地能力，不以修改范围文档替代功能实现。

## 本轮验证

| 验证 | 结果 |
|---|---|
| 前端回归 | 33 文件、254 项通过 |
| Rust 常规回归 | 221 项通过，24 项依赖本机服务的用例默认 ignored |
| 隔离普通 Redis | 7 项实际流程通过；3 项 Redis Stack 流程因未配置环境明确跳过 |
| CLI 临时 Redis | 9 项通过，含事务部分失败、超时、关闭和并发会话 |
| Sentinel / SSH | Sentinel 测试文件 2 项、SSH 专项 2 项通过 |
| 前端生产构建 | TypeScript / Vite 通过 |
| macOS 原生 release 构建 | `npm run tauri:build -- --no-bundle` 通过，生成 `src-tauri/target/release/redix`；未打包、签名或安装 |
| 范围与格式 | non-cloud、cargo fmt --check、git diff --check 通过 |

未完成 Redis Stack 实机验证、真实 TLS/mTLS 组合验证、Windows/Linux 原生验证、完整 Tauri 桌面交互端到端验证；SSH+CLI / Sentinel+CLI 的组合未单独做端到端测试。SSH 仅在 Unix 平台提供，Windows 明确拒绝。响应限制主要约束解析、IPC与展示，不能替代服务端资源限制或 Redis 客户端底层协议读取的内存限制。
