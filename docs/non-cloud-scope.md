# Redix 非 Cloud 范围

本文档是人工审查边界，不属于生产代码扫描输入。生产检查只读取 `src/`、`src-tauri/src/` 和 `package.json`，因此本文件中的排除项说明不会被误判为产品入口。

## 允许项

- 本地 Redis Standalone TCP 连接和连接配置管理。
- 使用系统钥匙串保存本地连接密码；前端 DTO 和连接列表不暴露密码。
- Browser 使用 `SCAN`、`MATCH`、`COUNT` 分页列出键，并读取键类型、TTL 和值。
- Browser 支持新增键、重命名、批量删除、元数据刷新、类型过滤、显式刷新和校验后的本地 JSON 导入导出。
- String、Hash、List、Set、Sorted Set、Stream 的基础读取、编辑、删除和 TTL 操作；Stream 单次读取最多 500 条记录。
- Stream Consumer Group 支持创建/删除 Group、读取消费者与 Pending 列表、确认 Pending 条目和删除消费者；Pending 单次最多读取 500 条。
- RedisJSON 根文档的读取和编辑；未安装 RedisJSON 模块时返回稳定的 `UNSUPPORTED_DATA_TYPE` 错误。
- Workbench 在已打开的本地连接上执行单条或多条 Redis 命令，并展示结构化结果、Raw/Text/JSON 格式、复制入口和遇错继续策略。
- Workbench 命令目录是 Rust 内置静态 DTO；历史按连接写入版本化 `workbench-history.json`，不使用 `localStorage`，AUTH、HELLO、ACL、CONFIG 命令族不落盘。
- Database 工作区读取本地实例与数据库键空间概览，并支持安全的数据库切换；指标不可用时按字段降级，不暴露 Redis 原始错误。
- Instance 详情按 INFO 分组展示客户端、内存、统计、持久化、复制指标及 commandstats；全部为当前连接的只读请求。
- Database Analysis 仅支持当前本地 Standalone 数据库的显式触发扫描：使用 `SCAN` 与固定批次 pipeline 汇总键空间、内存和过期时间；默认上限为 100000 个键，只读且不落盘，不会因连接或进入页面自动开始。
- Query Library 使用版本化 `query-library.json` 保存普通 Redis 命令，支持新增、编辑、删除、搜索和回填 Workbench；敏感命令不保存。
- 设置使用版本化 `settings.json` 保存主题、结果格式、Browser 扫描数量和批量命令遇错策略，Rust 与前端均执行范围校验。
- Slow Log 支持读取、清空和 `slowlog-max-len`/`slowlog-log-slower-than` 配置；Redis 回复解析兼容 RESP2 数组和 RESP3 Map。
- Pub/Sub 支持 Standalone channel/pattern 订阅、发布、停止和实时 Tauri 事件；每个连接最多一个订阅会话，前端最多保留 5000 条消息，并在关闭/切库/卸载时清理任务。
- Profiler 支持 Standalone 独立 `MONITOR` socket、启动/停止、实时命令事件和前端展示；每个连接最多一个会话，前端最多保留 10000 条事件，并在关闭/切库/卸载时清理任务；启动前显示性能风险提示。
- React/Tauri 本地 UI、前端测试、Rust 单元测试和本地构建工具链。

## 明确排除项

- Redis Cloud、Azure Managed Redis、RDI、AI、Telemetry 及其他云托管 Redis 产品。
- 云登录、云账户、云 SDK、云 API、云端点和云数据库发现。
- Cluster、Sentinel、拓扑发现或拓扑 fan-out、TLS、SSH、远程托管实例和云资源管理。
- Redis 模块专用数据类型、模块查询、模块可视化和模块编辑器（RedisJSON 根文档和 Stream 基础能力除外）。
- Stream 实时消费、阻塞式 `XREADGROUP`、`XCLAIM`/`XAUTOCLAIM`、Claim 流程、Profiler 日志文件/历史持久化/拓扑 fan-out 和超过 500 条记录的分页编辑。
- Monaco、远程插件运行时和云端命令目录。
- 其他未实现的运营分析能力和模块专用编辑器；Slow Log / Pub/Sub / 基础 Profiler 已按本文件允许项实现。

## 人工审查清单

- [x] 生产扫描目录固定为 `src/`、`src-tauri/src/`、`package.json`，不包含文档范围说明。
- [x] 入口词扫描大小写不敏感，命中后打印文件和词并以非零状态退出。
- [x] Browser 代码路径使用 `SCAN` 分页，没有加入 `KEYS` 命令。
- [x] 未加入云 SDK、云端点、云登录、云账户模型或云凭据存储。
- [x] 未引入 SQL，也没有在循环中查询 SQL。
- [x] Workbench 历史使用共享版本化 JSON 仓储，不保存密码、URI 或底层错误文本。
- [x] 现有前端和 Rust 测试仍需通过；真实 Redis、Tauri bundle 结果按实际环境记录。

## 测试命令

```bash
npm run check:non-cloud
npm run test:frontend
npm run build
npm run test:rust
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
CARGO_NET_OFFLINE=true cargo test --manifest-path src-tauri/Cargo.toml
npm run tauri:build
git diff --check
```

带真实 Redis 的集成测试需要显式设置 `REDIX_TEST_REDIS_URL`，并使用测试文件要求的 ignored 参数；未启动 Redis 时不能把 ignored 或未执行写成通过。
