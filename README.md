# Redix

Redix 是一个面向自管理 Redis 的桌面客户端，使用 Rust + Tauri + React 构建。当前版本提供连接管理（含导入导出、Standalone、Sentinel、Cluster 与 SSH/TLS 组合）、Browser 键浏览与编辑、RedisJSON 根文档与路径级操作、RedisSearch 索引管理与有限查询、Redis Array/Vector Set 模块数据操作、Stream Consumer Group 观察、数据库/实例概览、Query Library、本地设置、Slow Log、Pub/Sub、基础 Profiler，以及 Workbench 和 CLI。

## 前置条件

- Node.js 20+ 和 npm。
- Rust stable、Cargo，以及 Tauri 所需的系统依赖。
- 一个可访问的本地 Redis Standalone 实例，默认可使用 `redis://127.0.0.1:6379`。
- 若 Redis 开启认证，在连接表单中填写密码；密码只交给本地后端并由系统钥匙串保存，连接列表不回显密码。

## 安装与开发

```bash
npm install
npm run dev
```

`npm run dev` 启动前端开发服务器。需要运行桌面壳时使用：

```bash
npm run tauri:dev
```

## 测试、检查与构建

```bash
npm run check:non-cloud
npm run test:frontend
npm run test:rust
npm run test:redis:local # 需要 redis-server；自动创建并清理隔离实例
npm run test:redis:cluster # 需要 redis-server 与 redis-cli；自动创建并清理隔离三主节点
npm run build
npm run tauri:build
```

`check:non-cloud` 只扫描产品源码目录和 `package.json`，不扫描 README、设计文档或范围说明。它用于阻止非本地产品入口意外进入代码和菜单文案。

Browser 使用 Redis `SCAN` 分页浏览键，不使用阻塞式全量键枚举。Cluster 跨 primary 的完整遍历仅覆盖当前键 DTO 可表示的 UTF-8 键名；节点返回二进制键名时，该节点以可重试失败返回并保留游标，不能据此承诺任意二进制键空间都能完成遍历。Workbench 只在当前本地连接上执行用户输入的 Redis 命令。

Browser 支持新增键、重命名、批量删除、元数据刷新、类型过滤、显式刷新以及校验后的本地 JSON 导入导出；基础数据类型支持 String、Hash、List、Set、Sorted Set、Stream。Hash、List、Set、Sorted Set 使用有界分页和字段/成员/索引级增量写入，不用当前页重建整个键；Stream 使用有界 ID 范围分页，并支持显式添加和删除消息。Stream 详情还支持 Consumer Group 的创建/删除、消费者与 Pending 列表、Pending 确认、消费者删除和显式 XCLAIM 转移。转移需要选择消息并指定目标消费者、最小空闲时间，只操作满足条件的 Pending 消息，不启用 FORCE 或自动消费。RedisJSON 同时支持根文档编辑，以及路径级读取、保存、删除和数组追加。连接级模块能力通过 `MODULE LIST` 探测并按 session 缓存；探测失败或未检测到 RedisJSON/RedisSearch 时，不会阻断普通 Browser 流程，对应的路径编辑器或 Search / Query 工作区会稳定降级为不可用提示。RedisSearch / Query 工作区在 Search 2.0+ 可用时支持 `FT._LIST`、`FT.CREATE`、`FT.INFO`、`FT.DROPINDEX`、Hash/JSON 索引和有限的 `FT.SEARCH ... LIMIT` 查询（默认 NOCONTENT，可开启文档字段结果表），以及 typed `FT.AGGREGATE` LOAD/GROUPBY/REDUCE/SORTBY/LIMIT 查询；查询文本不持久化；Browser 的 Hash/JSON 键详情会显示匹配的索引摘要。

检测到对应命令集后，Browser 还支持 Redis Array 的连续/稀疏创建、范围读取、扫描、单元格编辑、追加、按索引或区间删除、ARGREP 搜索和 AROP 聚合；Vector Set 支持受限维度的元素创建与批量添加、分页浏览、向量/属性读取与编辑、FP32 向量下载、VSIM 相似度查询和元素删除。两类模块都通过 typed IPC 接入，模块缺失时只禁用对应类型和详情操作，不影响普通 Redis 键浏览；单次批量与响应大小均有固定上限。

Workbench 支持本地内置命令目录、命令前缀提示、多行批量执行、遇错继续策略、Raw/Text/JSON 结果格式和复制。命令历史按连接保存到应用数据目录的版本化 JSON 文件；AUTH、HELLO、ACL、CONFIG 命令族不会写入历史，也不使用 `localStorage`。

Database 工作区提供服务器版本、运行模式、连接数、内存、命令量、命中率和已加载模块等只读概览，并展示数据库键空间统计；实例详情按 INFO 分组展示客户端、内存、统计、持久化与复制指标，并提供 commandstats 命令统计。数据库切换成功后才更新当前连接配置。Database Analysis 是显式触发的只读工具：仅在用户点击“开始分析”后，以 `SCAN` 加固定批次 pipeline 汇总当前数据库；默认最多处理 100000 个键，也不会在连接后自动开始分析。结果默认不落盘；用户可在键名敏感提示后显式保存到版本化本机历史，按连接和数据库隔离，支持查看、删除及相同参数的观察值比较。Query Library 使用应用数据目录中的版本化 `query-library.json` 保存普通 Redis 命令，支持新增、编辑、删除、搜索和回填 Workbench，AUTH、HELLO、ACL、CONFIG 命令族不会保存。设置使用 `settings.json` 持久化主题、结果格式、Browser 扫描数量和批量命令错误策略。

运维观察工作区在 Standalone/Sentinel 上提供 Slow Log 的读取、清空和 `slowlog-*` 配置，独立 Pub/Sub channel/pattern 订阅、发布和实时消息流，以及基于独立 `MONITOR` socket 的 Profiler 实时命令流。Cluster 的 typed Slow Log、Pub/Sub 和 Profiler 均在网络操作前稳定拒绝，因为当前 DTO/会话没有明确节点作用域；命令工作台仍可显式执行原生命令，但路由范围由 Redis 驱动决定，不承诺全拓扑语义。Pub/Sub 每个连接只保留一个可取消会话，前端最多缓存 5000 条消息；Profiler 前端最多缓存 10000 条事件；关闭连接、切换数据库或卸载页面时会清理后台任务。Profiler 启动前会提示 MONITOR 可能带来的性能影响，不自动保存日志文件或历史记录。支持筛选、暂停显示及显式导出：Profiler LOG、Pub/Sub JSON、Slow Log CSV/JSON。暂停只冻结显示，后台仍接收并保留有界缓存；导出包含当前筛选快照，可能带有敏感命令参数。

## 当前边界

本版本支持 Standalone、Sentinel 和 Cluster DB 0。Cluster 已支持普通命令的 slot 路由、跨 primary 的 UTF-8 键名完整 `SCAN`、16384 slot 拓扑摘要及 primary-only Database Analysis；Cluster+SSH、Cluster Pub/Sub、Cluster Profiler 和 typed Cluster Slow Log 明确不支持。拓扑 DTO 目前也没有 RedisInsight 目标中的每节点 version/mode/totalkeys 等额外指标，不能称为全量等价。SSH 由内置 `ssh2` transport 提供 Agent、Password、内存私钥或本机私钥文件认证；Standalone/Sentinel 可与 TLS 组合，Cluster+SSH 禁用。TCP 建连、SSH 握手和远端认证共享 6 秒建连预算，超时或调用方取消会关闭已建立且仍在初始化的自有 socket；Agent 按返回顺序最多尝试 32 个身份且不重置该预算。但锁定的 libssh2 在 Unix/Windows 本地 Agent IPC 中可能同步阻塞，当前进程内接口无法可靠打断该本地等待；卡住时仍可能占用 worker、permit 和 CLI 生命周期锁，不能视为全部 SSH 认证均可超时取消。普通连接导出不包含密码、证书正文、SSH 私钥、口令或 SSH 本机路径。

文中所述响应大小上限约束的是已解码结构、解析、typed IPC 或展示层；当前没有 Redis 传输层原始 RESP 字节/分配上限，服务端超大回复仍可能在解析拒绝前占用内存。

Redis Cloud、Azure Managed Redis、RDI、AI/Copilot、Telemetry/Analytics、远程插件与插件市场永久排除；没有云登录、云账户、云数据库发现、云 SDK 或 SQL。当前只收口六批路线中的连接与拓扑第一批，Browser 解码器、后台任务和其他矩阵差异仍在后续批次。

除已列出的 RedisJSON、RedisSearch、Array、Vector Set 和 Stream 能力外，其他模块专用数据编辑器、Monaco/插件运行时尚未实现；不能将这些差异视作已对齐。

## 本轮补齐能力（2026-09-01）

- Browser 支持平铺/按 `:` 前缀分层的键树切换；树只展示当前已扫描结果，不额外扫描全库。JSON 可按对象/数组折叠、定位并回填路径编辑器，不能安全表达的路径只读。
- Workbench 支持离线模块命令帮助、当前光标行补全、`#`/`//` 注释行、嵌套结果树/表格，以及按连接删除单条或清空历史。敏感命令继续不写历史，包括被引号包围或转义的命令名。
- Search 可显式开启文档字段结果；默认键名模式兼容旧行为。支持 RESP2/RESP3 回复和过期文档空内容；字段、文档、深度、单页和总响应均有限额，超限返回固定错误。
- Sentinel 配置种子节点、master name 和独立认证，支持失败种子回退及主节点 ROLE 校验；重新连接或切库时重新发现。正在运行的会话不会无缝迁移到新的主节点。Sentinel 密码只存系统钥匙串，普通导出不含密码。
- SSH 使用跨平台 `ssh2` transport，支持 Agent、Password、内存 PrivateKey 和本机 identity file；必须用 known_hosts 严格验证主机指纹，未知或变化的主机密钥失败。支持 Standalone/Sentinel 与 TLS 组合，并保留原目标 host 作为 TLS SNI；Cluster+SSH 禁用。本轮只在 macOS 跑过自动化测试，不能据 CI 配置宣称 Windows/Linux 或真实 sshd 已通过。
- 大集合和 Stream 详情直接进入分页端点；Hash/List/Set/Sorted Set 原位编辑保留未加载数据与 TTL，键消失或类型变化时拒绝写入；Stream 读写保留 Consumer Group 和 TTL。
- Search 增加受限聚合查询面板，不接受任意聚合管道；Database Analysis 增加显式本机历史和相同扫描参数比较，报告可能包含键名且不自动保存。
- Standalone/Sentinel CLI 通过专用持久 socket 保留多轮 MULTI/EXEC、WATCH 和 SELECT 状态，不影响 Browser 的数据库。Cluster CLI 使用共享路由池，只支持普通无会话状态命令；事务、数据库选择、认证/协议切换、订阅、复制、连接模式和 `SCRIPT DEBUG` 等 socket 状态命令会在发送前稳定拒绝，Workbench 的 single/batch 采用相同门控。关闭、离开页面或主连接切库会清理 CLI；每条命令超时 5 秒后断开且不重试。

完整对比与尚未覆盖的能力见 [功能差异矩阵](docs/redisinsight-feature-matrix.md)。这不是与 RedisInsight 的全量等价实现。
