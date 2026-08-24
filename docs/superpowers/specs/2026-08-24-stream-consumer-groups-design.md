# 本地 Redis Stream Consumer Group 设计

## 目标

对照 `/Users/ushopal/workspace/myself/RedisInsight` 的 Stream Browser 能力，在 Redix 的本地 Redis Standalone Browser 中补齐 Consumer Group 管理和 PENDING 观察。能力通过现有 Rust + Tauri + React typed IPC 接入，不引入 Redis Cloud、Cluster/Sentinel fan-out、实时消费或新的持久化仓储。

## 范围

本批包含：

1. 读取 Stream 的 Consumer Group 列表、消费者数量、Pending 数量和最后投递 ID。
2. 创建和删除 Consumer Group；创建时支持 `0`、`$` 或显式 Stream ID 作为 last-delivered-id。
3. 读取指定 Group 的消费者列表，包括 Pending 数量和 idle 毫秒数。
4. 读取指定 Group 的 PENDING 明细，包括 Entry ID、消费者、idle 毫秒数和投递次数。
5. 对勾选的 PENDING Entry 执行 `XACK`，以及删除指定消费者。
6. 在现有 Stream 键详情中增加 Consumer Groups 区域，提供刷新、创建、选择 Group、查看消费者/PENDING、确认和删除操作。

明确不包含：

- `XREADGROUP` 实时消费、阻塞读取、Consumer 在线运行任务和 Tauri event channel。
- `XCLAIM`/`XAUTOCLAIM` 转移 Pending、消息编辑和 Stream 消息批量导入。
- Cluster/Sentinel 多节点 Fan-out、TLS/SSH、云服务、模块专用能力和 SQL。

## 用户流程

### 查看

当 Browser 选中的键类型为 Stream 时，Key Details 在现有 Stream 编辑器下方显示 Consumer Groups。组件挂载或键切换时调用 `get_stream_consumer_groups`；没有 Group 时显示空状态。刷新只刷新 Group 列表，并清除当前 Group 的消费者/PENDING 视图。

选择 Group 后，前端分别请求 `get_stream_consumers` 和 `get_stream_pending_entries`，默认最多读取 100 条 Pending。消费者和 Pending 列表都按后端返回顺序展示；Pending 表支持逐行选择和“全选当前列表”。

### 创建/删除

创建表单要求 Group 名称非空，last-delivered-id 默认 `$`。成功后刷新 Group 列表并选中新 Group。删除前使用浏览器二次确认；成功后刷新列表并清除已删除 Group 的详情。创建、删除和刷新期间禁用当前区域按钮，不阻塞页面其他键编辑状态之外的错误展示。

### 确认/删除消费者

确认操作只对已勾选 Entry ID 调用一次 `XACK`，成功后刷新当前 Group 的 Group 摘要、消费者和 Pending 列表。删除消费者前二次确认，调用 `XGROUP DELCONSUMER`，成功后刷新当前 Group；删除消费者会由 Redis 决定其 Pending 归属，UI 不伪造 Pending 状态。

## 领域 DTO 与 IPC

Rust 与 TypeScript 保持 snake_case 字段：

```text
GetStreamConsumerGroupsInput { connection_id, key }
StreamConsumerGroup { name, consumers, pending, last_delivered_id }
CreateStreamConsumerGroupInput { connection_id, key, name, last_delivered_id }
DeleteStreamConsumerGroupInput { connection_id, key, name }
GetStreamConsumersInput { connection_id, key, group }
StreamConsumer { name, pending, idle_ms }
GetStreamPendingEntriesInput { connection_id, key, group, count, consumer }
StreamPendingEntry { id, consumer, idle_ms, deliveries }
AcknowledgeStreamPendingEntriesInput { connection_id, key, group, entries }
DeleteStreamConsumerInput { connection_id, key, group, consumer }
```

Tauri request-response commands：

- `get_stream_consumer_groups` → `StreamConsumerGroup[]`
- `create_stream_consumer_group` → `void`
- `delete_stream_consumer_group` → `u64`（Redis 实际删除数量）
- `get_stream_consumers` → `StreamConsumer[]`
- `get_stream_pending_entries` → `StreamPendingEntry[]`
- `acknowledge_stream_pending_entries` → `u64`
- `delete_stream_consumer` → `u64`

所有输入在 Rust 校验连接 ID、键名、Group/consumer 名称和 ID 列表；Group/consumer/name 单值最大 256 字符，Pending count 最大 500，ack entries 最大 500。空字符串、空 entries、越界 count 返回 `INVALID_CONNECTION`。底层 Redis 错误统一映射为现有固定错误码，不把原始命令或连接信息返回给前端。

## Redis 命令与解析

服务只通过当前已打开连接执行：

```text
XINFO GROUPS key
XGROUP CREATE key group last-delivered-id
XGROUP DESTROY key group
XINFO CONSUMERS key group
XPENDING key group - + count [consumer]
XACK key group entry-id [entry-id ...]
XGROUP DELCONSUMER key group consumer
```

解析器兼容 Redis RESP2 的 Array/嵌套 Array、RESP3 的 Map/Attribute 包装。`XINFO` 的键值字段按字段名读取而不是依赖固定位置；缺少必要字段、类型不匹配或 Pending 明细结构异常时返回 `COMMAND_FAILED`。解析失败不回显原始 Redis 响应。

## 前端生命周期与安全

Consumer Group 组件只存在于当前 Stream 键详情；连接切换、键切换和组件卸载时取消未完成请求的 UI 更新，不在后台保留任务。调用顺序通过 operation token 和 connection/key 身份校验，旧请求不能覆盖新键详情。

Pending 表只展示 Redis 返回的 ID、consumer、idle 和 delivery count；React 默认转义命令/键内容。创建/删除/ack/删除 consumer 的错误通过现有固定 `browserErrorMessage` 映射展示，不展示底层 Redis 文本。

## 测试与验收

- Domain 测试覆盖输入校验、Group/consumer/Pending DTO 序列化边界。
- Redis parser 单元测试覆盖 RESP2、RESP3 Map/Attribute、malformed response 和缺失字段。
- Redis service 测试覆盖未打开连接、命令错误映射和固定返回值；真实 Redis Consumer Group 流程保留显式 ignored，不在普通测试中改变用户实例。
- Command/bridge 测试覆盖 7 个命令的 snake_case 名称和 `{ input }` 参数包装。
- React 测试覆盖加载/刷新、创建、删除确认、Group 切换、Pending 勾选/全选、XACK、删除 consumer、旧请求过滤和错误提示。
- 交付前运行 Rust 全量测试、前端全量测试、生产构建、`check:non-cloud`、`cargo fmt --check` 和 `git diff --check`。
