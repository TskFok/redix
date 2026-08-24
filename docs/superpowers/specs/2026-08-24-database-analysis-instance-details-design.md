# Database Analysis 与 Instance 详情设计

## 1. 背景

当前 Redix 已提供本地 Redis Standalone 的连接管理、Browser 键浏览和编辑、Workbench、Slow Log、Pub/Sub、Profiler、Stream Consumer Group、数据库概览、Query Library 和设置。对照 `/Users/ushopal/workspace/myself/RedisInsight`，当前最明显的本地缺口是：

1. 没有按需分析整个逻辑数据库的键数量、内存、类型、命名空间、长度和过期分布。
2. 实例页只有基础概览，没有按 `INFO` 分组的客户端、内存、命令统计、持久化和复制详情。
3. 没有与目标项目相近的独立 Database Analysis 工作区。

本设计只处理上述第一批能力，继续使用现有 Rust + Tauri 2 + React/Vite 架构，不复制 RedisInsight 的 Electron、NestJS、Redux 或图表运行时。

## 2. 目标与边界

### 2.1 本批目标

- 增加一个显式触发的 Database Analysis 工作区，生成当前逻辑数据库的一次性只读分析报告。
- 使用 Redis `SCAN` 游标遍历键，按批次用 pipeline 获取键类型、内存、TTL 和可计算长度，避免 `KEYS` 和前端逐键 IPC。
- 报告包含总键数、总内存、按类型汇总、按命名空间汇总、Top Keys、过期分布和扫描进度/截断状态。
- 扩展现有 Database/Instance 工作区，展示 `INFO` 中的客户端、内存、命令、持久化、复制和模块详情。
- Redis 版本、权限或配置导致单项指标不可用时局部降级，不影响同一报告的其他指标。
- 通过 typed IPC、固定错误码和 TDD 测试覆盖 Rust、Tauri command、TypeScript bridge、React 页面与真实 Standalone Redis 流程。

### 2.2 非目标

- 不实现 RedisInsight 的推荐规则、推荐投票、分析历史仓储或云端分析 API。
- 不实现 Cluster/Sentinel 多节点 fan-out、TLS/SSH、Azure Managed Redis、RDI、Redis Cloud、AI/Copilot、Telemetry 或远程插件。
- 不自动在打开连接或进入页面时扫描全库；用户必须主动点击“开始分析”。
- 不使用 `KEYS`、阻塞式全量枚举或 SQL；也不在循环遍历中查询 SQL。
- 不引入第三方图表库、Monaco、本地 HTTP 服务或新的数据库存储。
- 不改变现有 Browser 分页、Workbench 命令执行、Slow Log、Pub/Sub、Profiler 和 Stream Consumer Group 行为。

## 3. 用户流程

### 3.1 实例详情

用户打开连接后进入现有 Database 工作区。页面保留当前数据库概览和切换能力，新增“实例详情”区域：

1. 页面加载时请求一次 `get_instance_details`。
2. 按 Server、Clients、Memory、Stats、Persistence、Replication、Modules 分组显示指标。
3. 可点击“刷新”重新读取详情；刷新期间只禁用实例详情区域，不阻塞 Browser 或 Workbench。
4. `null` 指标显示“不可用”，固定错误只显示安全的用户文案。
5. 连接 ID 或数据库切换变化时，丢弃上一请求的结果；组件卸载时不再更新 React 状态。

### 3.2 Database Analysis

新增独立导航项“数据库分析”，只对当前已打开连接可用。

1. 用户填写可选的键名 pattern、命名空间分隔符和最大扫描键数；默认 pattern 为 `*`，分隔符为 `:`，最大扫描键数为 100000。
2. 点击“开始分析”后，前端以一次 typed IPC 请求提交输入；页面显示 loading、连接信息和本次扫描参数。
3. Rust 使用 SCAN 游标循环读取键名，按固定大小批次执行元数据 pipeline，边扫描边聚合，不把全部键值加载到内存。
4. 达到 `max_keys` 且 Redis 游标仍未回到 0 时停止扫描并将 `progress.truncated` 设为 `true`；游标自然返回 0 时即使处理数量等于上限也标记为完整报告。
5. 成功后显示摘要卡、按类型统计、命名空间表、Top Keys 表和 TTL 分布表；报告只保存在当前页面内，不落盘。
6. 连接切换、页面卸载或新分析开始后，旧报告不能覆盖新报告；本批只在前端丢弃过期响应，不承诺中断已经运行的后端 command，后续如需长时间取消另立 event-channel 设计。

## 4. 数据流与组件边界

```text
DatabasePage / DatabaseAnalysisPage
        │ typed wrapper
        ▼
Tauri command
        │ validate input + map error
        ▼
RedisService
        │ current Client + multiplexed connection
        ▼
SCAN cursor + batched pipeline + INFO
        │ safe DTO
        ▼
React state → cards / tables / status feedback
```

- `src/features/database` 继续负责数据库概览、数据库切换和实例详情展示。
- `src/features/database-analysis` 负责分析输入、加载状态、报告展示和请求序号保护。
- `src/lib/tauri.ts` 只暴露 typed wrapper，不在 React 层拼 Redis 命令。
- `src-tauri/src/domain/database.rs` 保留现有概览 DTO；新增 `database_analysis.rs` 保存分析输入、报告、聚合项和校验。
- `src-tauri/src/redis/connection_manager.rs` 继续管理当前连接和连接生命周期；分析的批量扫描实现放入 `src-tauri/src/redis/database_analysis.rs`，避免继续膨胀已有键 CRUD 文件。
- `src-tauri/src/commands/database.rs` 暴露实例详情和数据库分析 command，命令层只做输入校验、调用 service 和错误映射。

## 5. 领域模型

### 5.1 实例详情

在现有 `InstanceOverview` 之外增加可选分组 DTO：

```text
InstanceDetails {
  overview: InstanceOverview,
  clients: ClientDetails,
  memory: MemoryDetails,
  stats: StatsDetails,
  persistence: PersistenceDetails,
  replication: ReplicationDetails,
  command_stats: CommandStat[],
}
```

所有数值字段使用 `Option<u64>`、`Option<i64>` 或 `Option<f64>`，文本字段使用 `Option<String>`。推荐字段包括：

- `ClientDetails`：connected、blocked、tracking、max clients。
- `MemoryDetails`：used、peak、rss、fragmentation ratio、allocator active/resident。
- `StatsDetails`：total commands、instantaneous ops、hits、misses、hit ratio、expired/evicted keys。
- `PersistenceDetails`：loading、RDB last save、RDB changes、AOF enabled、AOF rewrite state。
- `ReplicationDetails`：role、connected replicas、master link status、replication offset。
- `CommandStat`：command name、calls、usec、usec per call、rejected calls、failed calls。

Redis `INFO` 不提供或权限不足的字段保持 `null`；解析错误只影响对应字段，不把原始 INFO 文本返回前端。

### 5.2 分析报告

```text
AnalyzeDatabaseInput {
  connection_id: string,
  pattern: string,
  delimiter: string,
  max_keys: number
}

DatabaseAnalysisReport {
  database: number,
  pattern: string,
  delimiter: string,
  progress: AnalysisProgress,
  total_keys: AnalysisSummary,
  total_memory: AnalysisSummary,
  top_keys_by_length: AnalysisKey[],
  top_keys_by_memory: AnalysisKey[],
  top_namespaces_by_keys: NamespaceSummary[],
  top_namespaces_by_memory: NamespaceSummary[],
  expiration_groups: ExpirationGroup[]
}

AnalysisProgress {
  scanned: number,
  processed: number,
  max_keys: number,
  truncated: boolean
}

AnalysisSummary {
  total: number,
  observed: number,
  types: TypeSummary[]
}

TypeSummary { type: string, total: number }

AnalysisKey {
  key: string,
  key_type: string,
  length: number | null,
  memory_bytes: number | null,
  ttl_seconds: number | null
}

NamespaceSummary {
  namespace: string,
  keys: number,
  memory_bytes: number,
  types: TypeSummary[]
}

ExpirationGroup {
  label: string,
  keys: number,
  memory_bytes: number
}
```

输入限制：`connection_id` 非空；pattern 长度不超过 512；delimiter 长度为 1–8 个字符；`max_keys` 范围为 1000–1_000_000。前端默认提交 100000，用户可以提高上限并明确承担扫描耗时。

## 6. Redis 访问与聚合算法

### 6.1 实例详情

`get_instance_details` 在当前已打开连接上执行：

```text
INFO server
INFO clients
INFO memory
INFO stats
INFO persistence
INFO replication
INFO commandstats
MODULE LIST
```

命令通过一个或少量 request-response 请求取得，复用现有 `parse_info_sections`，新增 commandstats 和 module reply parser。模块列表只读取名称/版本等安全字段；模块原始响应和连接信息不回传。

### 6.2 数据库分析

分析使用 Redis `SCAN`：

```text
SCAN cursor MATCH pattern COUNT batch_size
```

其中 `batch_size` 为 Rust 内部固定值 500，不能由用户输入任意放大。每批键名先通过 pipeline 读取：

```text
TYPE key
MEMORY USAGE key
TTL key
```

随后只对基础类型和 Stream 追加类型长度命令：

```text
STRLEN key   / LLEN key / HLEN key / SCARD key
ZCARD key    / XLEN key
```

不支持的模块类型长度为 `null`，但仍可计入键数、类型、命名空间和可用的内存/TTL 汇总。Redis 删除了某个键或返回 `nil` 时，该键计入 `scanned` 但不计入 `processed`，不会中止整次分析。

聚合器只保留：

- 每个 Redis 类型的计数和可用内存总量。
- 每个命名空间的计数、内存和类型计数；命名空间取第一个 delimiter 之前的键名，未命名空间的键不进入命名空间表。
- Top 15 的 key length 和 Top 15 的 memory，使用固定大小的小顶堆/排序窗口，不保存全部键详情。
- 固定的过期分组：No Expiry、`<1 hr`、`1-4 Hrs`、`4-12 Hrs`、`12-24 Hrs`、`1-7 Days`、`>7 Days`。

这保证分析过程不会按键逐次调用 Tauri，也不会将全部键集合复制到前端。TTL 为 `-1` 的键进入 No Expiry，非负 TTL 按分组阈值统计；TTL 为 `-2` 表示扫描后键已消失，该键不进入过期分组。Redis command 失败统一映射为 `COMMAND_FAILED`；单键 metadata 失败按不可用字段处理，只有连接断开或批次协议解析失败才终止报告。

## 7. 前端界面

### 7.1 实例详情

现有 DatabasePage 增加实例详情面板：

- 顶部保留服务器版本、模式、角色和刷新按钮。
- 使用紧凑的指标卡展示客户端、内存、吞吐、命中率和持久化状态。
- 使用表格展示 commandstats 和 modules；长命令名、模块名可横向滚动但不造成页面级横向溢出。
- `null` 值显示“不可用”，0 值显示为 `0`，字节和比例使用已有格式化 helper。

### 7.2 数据库分析

新增 `DatabaseAnalysisPage`：

- 输入区：pattern、delimiter、max keys、开始分析。
- 状态区：当前连接、当前数据库、扫描键数、处理键数、是否截断、耗时。
- 摘要区：总键数、总内存、类型数量、可用内存覆盖率；`total_keys.total`/`observed` 使用键数，`total_memory.total` 使用字节数，`total_memory.observed` 使用成功取得 `MEMORY USAGE` 的键数。
- 结果区：类型表、命名空间表、Top Keys 表、过期分布条形列表。
- 空状态说明尚未运行；加载状态禁用重复提交；失败状态使用固定 `databaseErrorMessage`。
- 使用现有 CSS token 和原生 HTML 表格/条形元素，不引入图表包。

## 8. 错误、性能和安全

- 新增或复用 `INVALID_INPUT`、`CONNECTION_NOT_OPEN`、`COMMAND_FAILED`、`OPERATION_CANCELLED`；不增加带 Redis 原文的错误载荷。
- 分析默认最大 100000 个键，前端明确显示“结果可能已截断”；上限由 Rust 再次校验。
- 使用 SCAN 和 pipeline，不使用 `KEYS`；pipeline 只处理固定批次，避免单次请求产生无限大的 Redis 命令包。
- 分析不执行写命令、不修改 TTL、不保存键值、不生成持久化历史。
- 连接切换和页面卸载后，前端通过 request token 丢弃旧结果；本批不创建可长期运行的 RedisService 后台任务，也不把前端取消误报为后端扫描已停止。
- Instance `INFO` 和 `MODULE LIST` 只读；模块名称、命令统计和 key 名仅按 DTO 返回，不返回密码、URI 或底层异常文本。
- 继续遵守当前项目约束：不引入 SQL，不在循环中查询 SQL，不增加 Redis Cloud/Azure/RDI/AI/Telemetry/远程插件入口。

## 9. 文件与接口边界

预计文件变更：

- Create: `src-tauri/src/domain/database_analysis.rs`
- Create: `src-tauri/src/redis/database_analysis.rs`
- Create: `src/features/database-analysis/databaseAnalysisState.ts`
- Create: `src/features/database-analysis/DatabaseAnalysisPage.tsx`
- Create: `src/features/database-analysis/databaseAnalysis.test.tsx`
- Modify: `src-tauri/src/domain/database.rs`, `src-tauri/src/domain/mod.rs`
- Modify: `src-tauri/src/redis/connection_manager.rs`, `src-tauri/src/redis/mod.rs`
- Modify: `src-tauri/src/commands/database.rs`, `src-tauri/src/commands/mod.rs`, `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/error.rs`（仅在现有固定错误码不足时）
- Modify: `src/lib/types.ts`, `src/lib/tauri.ts`
- Modify: `src/features/database/databaseState.ts`, `src/features/database/DatabasePage.tsx`
- Modify: `src/App.tsx`, `src/app.smoke.test.tsx`, `src/styles.css`
- Modify: `src-tauri/tests/commands.rs` 或新增数据库分析命令测试
- Create/modify: `src-tauri/tests/database_analysis.rs`（parser/聚合/ignored Redis 流程）
- Modify: `README.md`, `docs/non-cloud-scope.md`, `findings.md`, `progress.md`

新增 typed command：

```text
get_instance_details(connection_id) -> InstanceDetails
analyze_database(input: AnalyzeDatabaseInput) -> DatabaseAnalysisReport
```

已有 `get_instance_overview` 和 `get_database_overview` 保持兼容；前端可以先请求概览，再按需请求详情或分析。

## 10. 测试与验收

### Rust

- DTO 校验覆盖空连接、pattern、delimiter、max_keys 边界。
- `INFO` parser 覆盖完整 section、缺失 section、无效数字、commandstats 和 module reply。
- 分析 parser/aggregator 覆盖 RESP2/RESP3 兼容值、nil metadata、类型统计、命名空间、Top 15、TTL 分组和 max_keys 截断。
- Redis service 覆盖未打开连接、固定错误映射和单键 metadata 不完整时继续处理。
- ignored 集成测试覆盖多类型键、TTL、命名空间、Stream 及真实 INFO 读取；普通测试不自动连接或修改用户实例。

### 前端

- Instance 详情加载、刷新、null 指标、错误和过期响应保护。
- Analysis 输入默认值、校验、开始/加载/成功/截断/失败状态和结果表格。
- 新导航入口、无连接禁用状态和现有工作区回归。

### 验收命令

```text
npm run test:frontend
npm run build
npm run check:non-cloud
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo test --manifest-path src-tauri/Cargo.toml
git diff --check
```

设置 `REDIX_TEST_REDIS_URL` 时，再显式运行 ignored 的真实 Standalone 集成测试。交付前不声称 Redis 模块、拓扑、TLS/SSH 或云能力已完成。

## 11. 实施顺序

1. 先为领域 DTO、INFO parser、分析聚合器和输入校验编写 RED 测试。
2. 实现实例详情 parser/service 和 Tauri command。
3. 实现分析 SCAN/pipeline service 和 Tauri command。
4. 增加 TypeScript 类型、typed bridge 与前端 state 测试。
5. 接入 Instance 详情 UI 和 Database Analysis 页面、导航与样式。
6. 执行前端/Rust/构建/非 Cloud/差异回归；根据结果修复，不扩展到下一批连接拓扑能力。
