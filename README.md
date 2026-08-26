# Redix

Redix 是一个面向本地 Redis Standalone 的桌面客户端 MVP，使用 Rust + Tauri + React 构建。当前版本提供连接管理（含导入导出与 Standalone TCP/TLS）、Browser 键浏览与编辑、RedisJSON 根文档与路径级操作、Stream Consumer Group 观察、数据库/实例概览、Query Library、本地设置、Slow Log、Pub/Sub、基础 Profiler，以及带本地命令目录、批量执行和安全历史的 Workbench，适合开发环境中的单机 Redis 实例。

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
npm run build
npm run tauri:build
```

`check:non-cloud` 只扫描产品源码目录和 `package.json`，不扫描 README、设计文档或范围说明。它用于阻止非本地产品入口意外进入代码和菜单文案。

Browser 使用 Redis `SCAN` 分页浏览键，不使用阻塞式全量键枚举。Workbench 只在当前本地连接上执行用户输入的 Redis 命令。

Browser 支持新增键、重命名、批量删除、元数据刷新、类型过滤、显式刷新以及校验后的本地 JSON 导入导出；基础数据类型支持 String、Hash、List、Set、Sorted Set、Stream。Stream 编辑最多读取 500 条记录；Stream 详情还支持 Consumer Group 的创建/删除、消费者与 Pending 列表、Pending 确认和消费者删除，不包含实时消费或 Claim。RedisJSON 同时支持根文档编辑，以及路径级读取、保存、删除和数组追加。连接级模块能力通过 `MODULE LIST` 探测并按 session 缓存；探测失败或未检测到 RedisJSON 时，不会阻断普通 Browser 流程，路径编辑器会稳定降级为不可用提示。

Workbench 支持本地内置命令目录、命令前缀提示、多行批量执行、遇错继续策略、Raw/Text/JSON 结果格式和复制。命令历史按连接保存到应用数据目录的版本化 JSON 文件；AUTH、HELLO、ACL、CONFIG 命令族不会写入历史，也不使用 `localStorage`。

Database 工作区提供服务器版本、运行模式、连接数、内存、命令量、命中率和已加载模块等只读概览，并展示数据库键空间统计；实例详情按 INFO 分组展示客户端、内存、统计、持久化与复制指标，并提供 commandstats 命令统计。数据库切换成功后才更新当前连接配置。Database Analysis 是显式触发的只读工具：仅在用户点击“开始分析”后，以 `SCAN` 加固定批次 pipeline 汇总当前数据库；默认最多处理 100000 个键，结果不落盘，也不会在连接后自动开始分析。Query Library 使用应用数据目录中的版本化 `query-library.json` 保存普通 Redis 命令，支持新增、编辑、删除、搜索和回填 Workbench，AUTH、HELLO、ACL、CONFIG 命令族不会保存。设置使用 `settings.json` 持久化主题、结果格式、Browser 扫描数量和批量命令错误策略。

运维观察工作区提供 Slow Log 的读取、清空和 `slowlog-*` 配置，独立 Pub/Sub channel/pattern 订阅、发布和实时消息流，以及基于独立 `MONITOR` socket 的 Profiler 实时命令流。Pub/Sub 每个连接只保留一个可取消会话，前端最多缓存 5000 条消息；Profiler 前端最多缓存 10000 条事件；关闭连接、切换数据库或卸载页面时会清理后台任务。Profiler 启动前会提示 MONITOR 可能带来的性能影响，不保存日志文件或历史记录。

## 当前边界

本版本明确只支持本地 Redis Standalone（含 TCP/TLS）。当前不支持 Redis Cloud、Azure Managed Redis、RDI、AI/Copilot、Telemetry、Cluster/Sentinel 或其他拓扑能力、SSH、Stream 实时消费、XCLAIM/XAUTOCLAIM、Search/Query、Vector/Array 等后续 Redis 模块专用工作区；Profiler 仅提供基础实时 MONITOR，不包含日志文件、历史持久化或拓扑 fan-out，也不包含云登录、云账户、云端点、云数据库发现、云 SDK 集成、远程插件或 SQL。

除 RedisJSON 第一批路径编辑外，其他模块专用数据编辑器、Monaco/插件运行时和远程托管实例管理同样不在本 MVP 范围内。
