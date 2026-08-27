# RedisInsight 非 Cloud：RedisSearch/Query 第一批设计

## 状态

- 日期：2026-08-27
- 参考项目：`/Users/ushopal/workspace/myself/RedisInsight`
- 实现项目：`/Users/ushopal/workspace/myself/redix`
- 状态：待用户审阅

## 背景

当前 Redix 已支持本地 Redis Standalone 的连接、Browser 基础数据类型、RedisJSON 受限路径操作、Database/Analysis、Slow Log、Pub/Sub、Profiler、Workbench、Query Library、Settings 和 Standalone TLS。参考 RedisInsight 仍有一个独立的本地 RedisSearch 能力面：Browser 中的索引搜索与索引管理，以及基于索引的查询结果展示。

本批只补齐 RedisSearch/Query 的本地能力，不复制 RedisInsight 的 Electron/NestJS/Redux/Monaco 实现，也不把 Cloud、Azure、AI、Telemetry、远程插件或拓扑连接引入当前项目。

## 目标

1. 对当前连接探测并缓存 RedisSearch 模块能力，模块不可用时不影响普通 Browser、Workbench 和 Database 工作区。
2. 提供索引列表、索引创建、索引删除、索引信息查看和有限分页查询。
3. 允许 Browser 通过 RedisSearch 查询键名，并把结果作为安全、受限的键摘要展示。
4. 保持 Rust Redis service、Tauri typed IPC 和 React feature 的边界；前端不直接构造或执行 Redis client 调用。
5. 对 RESP2/RESP3 常见返回形态进行归一化，未知可选字段忽略，必要字段缺失返回固定错误。

## 非目标与硬边界

- 不实现 Redis Cloud、Azure Managed Redis、RDI、云登录/账户/端点/数据库发现、AI/Copilot、Telemetry、远程插件或插件市场。
- 不实现 SSH、Sentinel、Cluster 或跨节点 fan-out；查询只作用于当前 Standalone 连接和当前数据库。
- 不实现 FT.AGGREGATE、FT.PROFILE、FT.EXPLAIN、拼写建议、语义搜索、后台索引同步或自动生成查询。
- 第一版不实现完整的 RediSearch schema 编辑器；创建索引只支持高频的 HASH/JSON、prefix 和 TEXT/TAG/NUMERIC/GEO/VECTOR 字段定义。
- 不把 RedisSearch 返回的任意 map/raw reply 直接暴露到前端；不使用 `KEYS`、SQL、shell 或本地脚本。
- 查询文本作为单独 Redis command argument 传给 Rust service，不在前端拼接后执行未经约束的命令字符串。
- 任何遍历不得查询 SQL；当前项目继续不引入 SQL 持久化。

## 参考项目证据

参考项目的 `api/src/modules/browser/redisearch` 暴露了以下本地 RedisSearch 入口：

- `FT._LIST`：索引列表；
- `FT.CREATE`：创建索引；
- `FT.INFO`：读取索引信息；
- `FT.SEARCH ... NOCONTENT LIMIT`：按索引查询键；
- `FT.DROPINDEX`：删除索引；
- 通过索引定义中的 key type 和 prefix 关联 Browser 键详情。

参考 UI 的 Browser 搜索面板在 RedisSearch 模块缺失或版本不满足时禁用 RedisSearch 搜索模式；目标项目还将索引信息、搜索结果和普通键浏览保持为相互独立的状态边界。当前 Redix 没有 Search/Query feature、Search command、索引 DTO 或对应 Tauri bridge。

## 方案比较

### 方案 A：直接移植 RedisInsight 的 RedisSearch 模块

不采用。目标实现依赖 NestJS controller、node-redis/ioredis、Redux 状态、序列化拦截器和 Electron 生命周期，直接移植会引入大量不兼容依赖，并破坏当前 Rust/Tauri 边界。

### 方案 B：Rust typed service + 独立 React Search/Query 工作区（推荐）

在现有 `RedisService` 和 capability snapshot 上增加独立 Search domain/service/commands，前端新增独立 Search/Query feature。优点是能复用连接、TLS、固定错误和请求 token 保护，同时把 Search 状态与 Browser 基础扫描解耦；代价是需要为 RESP2/RESP3 和 FT.INFO 的复杂返回写归一化 parser。

### 方案 C：只在 Workbench 中放开 FT.* 命令

不采用。虽然实现成本最低，但无法补齐索引管理、能力降级、分页结果、Browser 关联和结构化索引信息，也无法达到目标项目的本地用户流程。

## 选定架构

采用方案 B，按以下数据流实现：

```text
React Search/Query page
        │ typed bridge
        ▼
Tauri search commands
        │ validated DTO
        ▼
RedisService / redis::search
        │ current active Standalone client
        ▼
FT._LIST / FT.CREATE / FT.INFO / FT.SEARCH / FT.DROPINDEX
```

Capability snapshot 继续只存在当前连接 session 内，不写入 `connections.json`。连接打开、关闭、切换数据库或替换 client 时递增 generation 并清理缓存；探测失败只把 Search 状态标记为未知/不可用，不阻断连接打开。

## Domain 与 IPC 合同

### Capability

扩展现有 `ModuleCapabilities`：

```rust
pub struct ModuleCapabilities {
    pub modules: Vec<ModuleSummary>,
    pub json_supported: bool,
    pub json_version: Option<String>,
    pub search_supported: bool,
    pub search_version: Option<String>,
}
```

RedisSearch 模块名称兼容 `search`、`redisearch` 和 `RediSearch` 的大小写变体；版本只用于显示和最小版本提示，最低支持版本固定为 `2.0.0`，不根据模块名称猜测不存在的命令。真正执行时仍以 Redis 返回为准。

### Search DTO

以下 DTO 使用 Rust `Serialize/Deserialize` 和 snake_case 字段；前端只接收这些 DTO，不接收 `redis::Value`：

```rust
pub struct ListSearchIndexesInput {
    pub connection_id: String,
}

pub struct SearchIndexSummary {
    pub name: String,
}

pub struct ListSearchIndexesResult {
    pub indexes: Vec<SearchIndexSummary>,
}

pub enum SearchKeyType {
    Hash,
    Json,
}

pub enum SearchFieldType {
    Text,
    Tag,
    Numeric,
    Geo,
    Geoshape,
    Vector,
}

pub struct SearchIndexFieldInput {
    pub name: String,
    pub field_type: SearchFieldType,
}

pub struct CreateSearchIndexInput {
    pub connection_id: String,
    pub index: String,
    pub key_type: SearchKeyType,
    pub prefixes: Vec<String>,
    pub fields: Vec<SearchIndexFieldInput>,
}

pub struct SearchIndexInput {
    pub connection_id: String,
    pub index: String,
}

pub struct GetKeySearchIndexesInput {
    pub connection_id: String,
    pub key: String,
}

pub struct KeySearchIndexSummary {
    pub name: String,
    pub key_type: String,
    pub prefixes: Vec<String>,
}

pub struct SearchIndexAttribute {
    pub identifier: String,
    pub field_type: String,
    pub sortable: bool,
    pub no_index: bool,
}

pub struct SearchIndexInfo {
    pub index_name: String,
    pub key_type: String,
    pub prefixes: Vec<String>,
    pub attributes: Vec<SearchIndexAttribute>,
    pub num_docs: Option<u64>,
    pub num_terms: Option<u64>,
    pub num_records: Option<u64>,
    pub total_index_memory_bytes: Option<u64>,
}

pub struct SearchQueryInput {
    pub connection_id: String,
    pub index: String,
    pub query: String,
    pub offset: u64,
    pub limit: u32,
}

pub struct SearchKeyResult {
    pub key: String,
    pub key_type: String,
}

pub struct SearchQueryResult {
    pub total: u64,
    pub offset: u64,
    pub next_offset: Option<u64>,
    pub max_results: Option<u64>,
    pub keys: Vec<SearchKeyResult>,
}
```

`SearchKeyType` 和 `SearchFieldType` 序列化为稳定的小写字符串。若 `FT.INFO` 某个可选统计项不能转换为数字，保留 `None`；索引名、key type、schema attribute 等必需结构缺失时返回 `COMMAND_FAILED`。

### RedisOperations 与 commands

`RedisOperations` 新增：

- `list_search_indexes(connection_id)`；
- `create_search_index(input)`；
- `get_search_index(input)`；
- `delete_search_index(input)`；
- `search_keys(input)`；
- `get_key_search_indexes(input)`。

对应 Tauri command 使用稳定 snake_case 名称：

- `list_search_indexes`；
- `create_search_index`；
- `get_search_index`；
- `delete_search_index`；
- `search_keys`；
- `get_key_search_indexes`。

每个 command 只负责输入校验、调用 service 和返回固定 DTO；Redis 错误统一映射为 `UNSUPPORTED_FEATURE`、`INVALID_INPUT`、`INVALID_CONNECTION` 或 `COMMAND_FAILED`，不回传 URI、密码、证书、原始服务器错误或原始查询结果中的底层错误文本。

## Redis command 与 parser 设计

### Capability 探测

复用现有 `MODULE LIST` 读取路径，在严格 parser 中识别 RedisSearch 模块及版本。空模块列表是合法“无模块”；malformed top-level、缺少模块名或结构无法解析时返回 `COMMAND_FAILED`，而不是缓存成“没有 Search”。探测错误不会阻塞普通连接，但 Search 页面显示不可用/探测失败。

### 索引列表

使用 `FT._LIST`，结果限制为最多 500 个索引。由于 `FT._LIST` 只返回名称，列表页面展示名称；选择索引后再读取 `FT.INFO` 得到 key type 和 prefix，失败只影响该索引详情。

### 创建和删除

Rust service 使用 `redis::Cmd` 按 argument 添加命令参数，不使用字符串拼接。创建命令形态为：

```text
FT.CREATE <index> ON HASH|JSON [PREFIX <count> <prefix>...] SCHEMA <field> <type>...
```

创建前由 domain 校验索引名、字段名、字段数量、prefix 数量和字段类型；不暴露 `FILTER`、`LANGUAGE`、`STOPWORDS` 等高级 schema 选项。删除使用 `FT.DROPINDEX <index>`，默认保留原始 Redis 文档，不使用 `DD`。

### 索引信息

使用 `FT.INFO <index>`，解析：

- RESP2 的扁平 key/value 数组；
- RESP3 的 map；
- RESP3 Attribute 包装；
- `attributes` 内部的数组/map 混合结构。

只读取设计 DTO 中声明的字段；未知字段忽略。`attributes` 上限 256，prefix 上限 64，单个字符串上限 256 字节，完整解析响应上限 4 MiB。

### Browser 键关联索引

`get_key_search_indexes` 先读取当前键的 Redis type，再读取最多 500 个索引及其必要的 `FT.INFO` 定义，最后在内存中按 HASH/JSON 类型和 prefix 匹配。匹配过程不会为每个候选键单独访问 Redis；索引信息读取使用受限并发/批量策略，单个索引解析失败只跳过该索引。非 HASH/JSON 键返回空列表。

### 查询

使用：

```text
FT.SEARCH <index> <query> NOCONTENT LIMIT <offset> <safe_limit>
```

`query` 作为独立参数传入，不能改变命令结构。`limit` 固定为 `1..=200`，`offset` 最大 100000；如果服务器通过 `FT.CONFIG GET MAXSEARCHRESULTS` 暴露上限，service 将安全截断本次请求，并在 DTO 中返回 `max_results`。结果只保留 total 和键名，键类型通过单次受限 pipeline 的 `TYPE` 查询补充；单次结果最多 200 条。

`next_offset` 在还有结果时为 `offset + keys.len()`，没有更多结果时为 `None`。空查询结果是合法结果，不视为错误。查询失败返回固定 `COMMAND_FAILED`。

## 前端设计

新增 `src/features/search/`，包含：

- `SearchQueryPage.tsx`：索引管理和查询工作区；
- `searchState.ts`：能力状态、索引/详情/查询结果状态和错误归一化；
- `search.test.tsx`、`searchState.test.ts`：页面和状态回归。

App 导航新增“Search / Query”入口。未连接时入口与 Browser 一致保持禁用；已连接但 Search 不可用时可以进入页面并显示：

1. 正在探测：加载状态；
2. 模块缺失或版本不满足：可恢复的“不支持 RedisSearch”提示和返回 Browser 操作；
3. 能力可用：索引列表、刷新、创建、索引详情、删除和查询表单。

查询结果显示 total、当前页、key 名称、类型和下一页操作。创建/删除/查询期间按钮按请求状态禁用；删除必须确认；查询成功后才替换当前结果。索引或连接切换会递增 request token，旧响应不得覆盖新索引/新连接状态。组件卸载时不更新 React state。

Search 页面可以提供“在 Workbench 中查看命令”填充动作，但只回填可见文本，不自动执行；第一版不新增 Search 专用持久化历史，避免与现有 Query Library/Workbench 历史产生重复存储。

## 错误、降级与生命周期

- 连接未打开：返回 `INVALID_CONNECTION`/`CONNECTION_FAILED`。
- Search 模块缺失或 FT 命令不可用：返回 `UNSUPPORTED_FEATURE`，普通工作区不受影响。
- 输入超限或枚举非法：返回 `INVALID_INPUT`。
- RESP 结构缺失、命令失败或结果超限：返回 `COMMAND_FAILED`。
- 连接关闭/切换/替换时清理 Search capability cache；页面 request token 只接受当前 connection id 和 index 的响应。
- 创建成功后可刷新索引列表；删除成功后清理详情和查询结果。刷新失败不得覆盖最近一次成功的索引列表。
- 不对 Search 查询启用后台轮询；刷新由用户显式触发。

## 安全与性能边界

固定限制：

- 索引名、字段名、prefix：最多 256 字节；
- 查询文本：最多 4096 字节；
- 字段数量：1..=64；prefix 数量：0..=64；
- 索引列表：最多 500；索引属性：最多 256；
- 查询 offset：0..=100000；limit：1..=200；
- 单次返回键：最多 200；单次 Redis 响应：最多 4 MiB。

查询和索引操作都必须作用于当前 active client；不会保存密码、证书、查询结果或 capability snapshot 到连接 profile。Search query 文本不自动进入 Workbench 历史或 Query Library。

## 测试策略

遵循 RED → GREEN → REFACTOR：

1. 先为输入限制、Search capability 判定、RESP2/RESP3 parser、命令 adapter 和前端页面写失败测试，并确认失败原因是功能缺失。
2. 以最小 service/bridge/page 实现使定向测试通过，再补 malformed reply、空结果、能力不可用、删除确认、请求竞态和卸载清理。
3. 运行全量回归和生产构建。

至少覆盖：

- `MODULE LIST` 中 Search/RediSearch 名称和版本识别、空列表、malformed entry；
- FT._LIST 的 RESP2/RESP3/Attribute 解析和 500 条上限；
- FT.INFO flat array、map、attribute、缺少必需字段和未知可选字段；
- 查询 total、分页、空结果、MAXSEARCHRESULTS 截断和 200 条上限；
- 创建字段类型/数量/prefix 校验、删除保留原始 key；
- Search 模块不可用时普通 Browser 仍可加载；
- 前端能力 loading/unsupported/ready 三态、索引 CRUD、查询结果、旧响应丢弃和 loading 禁止重复提交；
- Tauri bridge 命令名和参数形状；
- 配置 `REDIX_TEST_REDIS_STACK_URL` 后的 ignored Redis Stack 索引/查询流程；未配置时明确记录 skipped，不把离线 parser 测试当作网络验收。

完成时运行：

```bash
npm run test:frontend
npm run build
npm run check:non-cloud
npm run test:rust
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
git diff --check
```

## 交付文件范围

预期新增/修改：

- `src-tauri/src/domain/module_capabilities.rs`、新增 Search domain 文件；
- `src-tauri/src/redis/connection_manager.rs`、新增 Search service 文件；
- `src-tauri/src/commands/search.rs`、`src-tauri/src/lib.rs`；
- `src/lib/types.ts`、`src/lib/tauri.ts` 及 bridge 测试；
- `src/features/search/*`、`src/App.tsx`、相关样式和测试；
- `README.md`、`docs/non-cloud-scope.md`、`task_plan.md`、`findings.md`、`progress.md`；
- Rust domain/command/integration 测试和 ignored Redis Stack 流程。

不修改参考项目，不引入 Cloud/AI/Telemetry/远程插件/SQL，不在本批实现 Vector Set、Array、SSH、Sentinel 或 Cluster。

## 验收标准

- RedisSearch 可用时，用户能够在当前 Standalone 数据库中查看索引、创建/删除索引、读取索引信息并按页查询键。
- RedisSearch 不可用或版本不兼容时，页面显示稳定降级状态，现有 Browser、Workbench、Database、JSON 和观察能力不回归。
- Rust/TypeScript DTO、Tauri command、RESP parser 和前端请求状态均有测试；全量测试、构建、非 Cloud 扫描、格式和 diff 检查有新鲜证据。
- 任何结果/错误都不泄露密码、证书、URI 或底层错误文本；实现中没有 Redis Cloud、Azure、RDI、AI、Telemetry、远程插件、SQL 或 `KEYS` 生产入口。
