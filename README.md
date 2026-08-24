# Redix

Redix 是一个面向本地 Redis Standalone 的桌面客户端 MVP，使用 Rust + Tauri + React 构建。当前版本提供连接管理、Browser 键浏览与编辑、基础 Stream/RedisJSON 根文档操作、数据库/实例概览、Query Library、本地设置、Slow Log、Pub/Sub、基础 Profiler，以及带本地命令目录、批量执行和安全历史的 Workbench，适合开发环境中的单机 Redis 实例。

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

Browser 支持新增键、重命名、批量删除、元数据刷新、类型过滤、显式刷新以及校验后的本地 JSON 导入导出；基础数据类型支持 String、Hash、List、Set、Sorted Set、Stream。Stream 编辑最多读取 500 条记录，不包含 Consumer Group 或实时订阅。RedisJSON 只编辑根文档；RedisJSON 模块不可用时会显示稳定的“不支持的数据类型”提示。

Workbench 支持本地内置命令目录、命令前缀提示、多行批量执行、遇错继续策略、Raw/Text/JSON 结果格式和复制。命令历史按连接保存到应用数据目录的版本化 JSON 文件；AUTH、HELLO、ACL、CONFIG 命令族不会写入历史，也不使用 `localStorage`。

Database 工作区提供服务器版本、运行模式、连接数、内存、命令量、命中率和已加载模块等只读概览，并展示数据库键空间统计；数据库切换成功后才更新当前连接配置。Query Library 使用应用数据目录中的版本化 `query-library.json` 保存普通 Redis 命令，支持新增、编辑、删除、搜索和回填 Workbench，AUTH、HELLO、ACL、CONFIG 命令族不会保存。设置使用 `settings.json` 持久化主题、结果格式、Browser 扫描数量和批量命令错误策略。

运维观察工作区提供 Slow Log 的读取、清空和 `slowlog-*` 配置，独立 Pub/Sub channel/pattern 订阅、发布和实时消息流，以及基于独立 `MONITOR` socket 的 Profiler 实时命令流。Pub/Sub 每个连接只保留一个可取消会话，前端最多缓存 5000 条消息；Profiler 前端最多缓存 10000 条事件；关闭连接、切换数据库或卸载页面时会清理后台任务。Profiler 启动前会提示 MONITOR 可能带来的性能影响，不保存日志文件或历史记录。

## 当前边界

本版本明确只支持本地 Redis Standalone。当前不支持 Redis Cloud、Azure Managed Redis、Cluster、Sentinel、TLS、SSH、Stream Consumer Group 或其他 Redis 模块专用编辑器；Profiler 仅提供基础实时 MONITOR，不包含日志文件、历史持久化或拓扑 fan-out，也不包含云登录、云账户、云端点、云数据库发现和云 SDK 集成。

模块专用数据编辑器、Monaco/插件运行时和远程托管实例管理同样不在本 MVP 范围内。
