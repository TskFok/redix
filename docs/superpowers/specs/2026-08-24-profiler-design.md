# 本地 Redis Profiler 设计

日期：2026-08-24

## 背景

RedisInsight 的 Profiler 使用独立 MONITOR 连接接收实时命令流。Redix 已经具备 Pub/Sub 的可取消事件任务、固定错误码和运维观察页面，因此下一批只补齐本地 Standalone 的基础 Profiler，不扩展到拓扑或云端能力。

## 范围

包含：

- `MONITOR` 独立 socket 的启动、停止和实时命令事件。
- 从 MONITOR 文本解析时间、数据库、来源和命令参数。
- 每个连接最多一个 Profiler 会话；启动新会话、关闭连接、重新打开连接和切换数据库时取消旧任务。
- Tauri typed commands 与 `redix://profiler/event`、`redix://profiler/status` 事件。
- 前端展示、暂停/停止和最多保留 10,000 条事件；页面卸载时清理监听器和会话。

不包含：

- 日志文件保存/下载、Profiler 历史持久化和远程日志。
- Cluster/Sentinel 多分片 fan-out、TLS/SSH、Redis Cloud/Azure、AI、Telemetry、远程插件。
- Redis 模块专用数据类型、SQL 查询和数据库循环查询。

## 数据与协议

Profiler 输入使用 `connection_id`、`session_id`。事件包含 `connection_id`、`session_id`、`time`、`args`、`source`、`database` 和 `received_at_ms`。状态事件只包含连接、会话、`running`/`stopped` 状态和固定错误码。

MONITOR 行按 Redis 常见格式解析：`<unix-seconds> [<db> <source>] <quoted-command>`。解析失败时丢弃该行，不把原始内容或连接信息写入错误消息。

## 生命周期

Rust `ProfilerManager` 保存每个连接的 `JoinHandle` 和会话标识。停止只取消匹配会话，缺失会话幂等成功；连接替换路径在写入新 Client 前取消旧任务。前端按连接和会话过滤事件，组件卸载先解除 listener，再异步请求停止。

## 风险提示

Profiler 会接收实例上的全部命令并可能影响 Redis 性能。UI 必须在启动按钮附近显示明确提示，默认不自动启动；不在前端或错误对象中展示 Redis 原始错误文本。

## 验收

- parser 单测覆盖正常命令、转义参数、非法行和固定错误边界。
- Rust service/command 测试覆盖未打开连接、停止幂等和生命周期入口。
- 前端测试覆盖启动/停止、事件过滤、10,000 条上限和卸载清理。
- `cargo test`、`cargo fmt --check`、`npm test`、`npm run build`、`npm run check:non-cloud` 通过。
- 真实 Redis MONITOR 测试只保留显式 ignored，不在未获授权时自动启动或写入生产实例。
