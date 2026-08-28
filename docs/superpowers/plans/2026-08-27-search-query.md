# RedisSearch / Query 第一批实现计划

> **执行记录：** 用户选择在当前 `main` 分支 Inline Execution；本计划已按 Task 1–7 逐项执行。步骤使用 checkbox (`- [ ]` / `- [x]`) 记录。

**Goal:** 在当前 Redix 的本地 Redis Standalone 连接上补齐 RedisSearch 索引管理、索引信息、有限分页查询和 Browser 键关联；RedisSearch 不可用时局部降级，既有 Browser、JSON、Workbench、Database 和运维工作区保持可用。

**Architecture:** Rust domain DTO 与 `RedisOperations` 负责输入限制和固定错误边界，`redis::Cmd` 负责安全地按参数执行 `FT.*` 命令，`MODULE LIST` 能力快照负责当前连接级 Search 兼容性判断；Tauri commands 暴露 typed IPC；React 独立 `src/features/search/` 工作区管理索引/详情/查询状态，Browser 只消费受限的索引摘要。

**Tech Stack:** Rust 2021、Tauri 2、`redis` 1.5 async client、Serde、Tokio、React 19、TypeScript、Vitest、Testing Library。

**Spec:** `docs/superpowers/specs/2026-08-27-search-query-design.md`

## Global Constraints

- 默认继续在当前 `main` 分支修改，不创建分支；每个阶段提交信息使用简体中文。
- 只作用于当前已打开的 Standalone client 和当前数据库，不加入 Cloud、Azure、RDI、AI、Telemetry、远程插件、SQL、SSH、Sentinel、Cluster、Vector Set 或 Array 能力。
- 所有 Search 请求经过 typed Tauri IPC；前端不直接执行 Redis 命令，不把查询文本拼成可执行命令字符串。
- 生产代码不使用 `KEYS`，不写 SQL，也不在任何循环中查询 SQL；索引关联使用受限 `FT.INFO` 批次，查询结果的键类型使用单次 pipeline。
- Redis 原始错误、URI、密码、证书、原始 `redis::Value` 和任意未约束 map 不进入前端 DTO；错误只使用现有固定错误或新增 `UNSUPPORTED_FEATURE`。
- 固定限制不可放宽：索引/字段/prefix 最多 256 字节，查询最多 4096 字节，字段 1..=64，prefix 0..=64，索引最多 500，属性最多 256，offset 0..=100000，limit 1..=200，单次键结果最多 200，单次 Redis 响应估算最多 4 MiB。
- 每个实现任务均按 RED → GREEN → REFACTOR 执行；先运行会因功能缺失而失败的定向测试，再写最小实现，再运行定向测试和格式检查。

## File Map

| 文件 | 责任 |
| --- | --- |
| `src-tauri/src/domain/search.rs` | Search 输入、索引、查询 DTO、固定上限和校验 |
| `src-tauri/src/domain/module_capabilities.rs` | Search 模块名称、版本和最低版本判断 |
| `src-tauri/src/error.rs` | `UNSUPPORTED_FEATURE` 固定错误码和文案 |
| `src-tauri/src/redis/search.rs` | `FT._LIST`、`FT.INFO`、`FT.SEARCH` RESP2/RESP3 parser、命令构造和回复大小限制 |
| `src-tauri/src/redis/connection_manager.rs` | Search service 实现、能力缓存、pipeline 和当前连接生命周期 |
| `src-tauri/src/commands/search.rs`、`src-tauri/src/commands/mod.rs`、`src-tauri/src/lib.rs` | Tauri command 注册与 typed service 适配 |
| `src-tauri/tests/domain.rs`、`src-tauri/tests/commands.rs`、`src-tauri/tests/redis_integration.rs` | domain、command 注册和可选 Redis Stack 流程回归 |
| `src/lib/types.ts`、`src/lib/tauri.ts` | TypeScript DTO 和 IPC wrapper |
| `src/features/search/searchState.ts` | Search 能力、版本、请求结果和错误状态纯函数 |
| `src/features/search/SearchPage.tsx` | 索引管理、详情和分页查询工作区 |
| `src/features/browser/KeyDetails.tsx` | Browser 详情中的索引关联摘要 |
| `src/App.tsx`、`src/features/browser/BrowserPage.tsx`、`src/features/browser/KeyDetails.tsx`、`src/styles.css` | 导航、Browser 注入和样式 |
| `src/lib/tauri.test.ts`、`src/features/search/searchState.test.ts`、`src/features/search/search.test.tsx`、`src/features/search/KeySearchIndexes.test.tsx`、现有 Browser/App smoke tests | typed bridge、纯状态、页面、Browser 竞态和回归 |
| `README.md`、`docs/non-cloud-scope.md`、`task_plan.md`、`findings.md`、`progress.md` | 交付能力、边界和验证证据 |

---

## Task 1: 建立 Search domain DTO、校验和 capability 合同

**Files:** 新建 `src-tauri/src/domain/search.rs`；修改 `src-tauri/src/domain/mod.rs`、`src-tauri/src/domain/module_capabilities.rs`、`src-tauri/src/error.rs`；修改 `src-tauri/tests/domain.rs`。

- [ ] **RED：先写领域失败测试。** 在 `src-tauri/tests/domain.rs` 增加以下测试，先引用尚不存在的 Search 类型和 `AppError::UnsupportedFeature`：

  - `search_create_input_rejects_empty_index_and_duplicate_fields`：空 `connection_id` 返回 `InvalidConnection`；空 `index`、重复 field name 返回 `InvalidInput`。
  - `search_create_input_enforces_field_and_prefix_limits`：65 个字段、65 个 prefix、256 字节以上的字段名和 prefix 各返回 `InvalidInput`；1 个合法 field 加 64 个 prefix 通过。
  - `search_query_input_enforces_query_offset_and_limit`：空 query、4097 字节 query、offset 100001、limit 0、limit 201 返回 `InvalidInput`；`offset=100000, limit=200` 通过。
  - `search_capabilities_recognize_search_module_names_and_versions`：`Search`、`RediSearch`、`redisearch` 都设置 `search_supported=true`；版本保留为 `Some("2.6.11")`；没有 Search 模块时为 false；`search_version_supported(None)` 和 `Some("1.6.0")` 为 false，`Some("2.0.0")` 与 `Some("2.10.0")` 为 true。
  - `unsupported_feature_serializes_to_stable_ipc_error`：`serde_json::to_value(AppError::UnsupportedFeature)` 精确等于 `{ "code": "UNSUPPORTED_FEATURE", "message": "当前 Redis 功能不可用" }`。

- [ ] 运行 RED：

  ```bash
  cargo test --manifest-path src-tauri/Cargo.toml --test domain search_ -- --nocapture
  ```

  预期失败原因是 Search DTO、Search capability 字段和错误枚举尚未定义；若失败原因是既有测试或依赖环境，先记录到 `task_plan.md`，不得把环境错误当作 RED 证据。

- [ ] **GREEN：实现 `search.rs`。** 使用 `Serialize`、`Deserialize`、`Clone`、`Debug`、`PartialEq`、`Eq`，枚举使用 `#[serde(rename_all = "lowercase")]`，字段使用 snake_case。精确提供：

  ```rust
  pub const REDISEARCH_MIN_VERSION: &str = "2.0.0";
  pub const MAX_SEARCH_NAME_BYTES: usize = 256;
  pub const MAX_SEARCH_KEY_BYTES: usize = 512;
  pub const MAX_SEARCH_QUERY_BYTES: usize = 4096;
  pub const MAX_SEARCH_ATTRIBUTES: usize = 256;
  pub const MAX_SEARCH_FIELDS: usize = 64;
  pub const MAX_SEARCH_PREFIXES: usize = 64;
  pub const MAX_SEARCH_INDEXES: usize = 500;
  pub const MAX_SEARCH_OFFSET: u64 = 100_000;
  pub const MAX_SEARCH_PAGE: u32 = 200;
  pub const MAX_SEARCH_RESPONSE_BYTES: usize = 4 * 1024 * 1024;
  ```

  提供 `ListSearchIndexesInput`、`SearchIndexSummary`、`ListSearchIndexesResult`、`SearchKeyType { Hash, Json }`、`SearchFieldType { Text, Tag, Numeric, Geo, Geoshape, Vector }`、`SearchIndexFieldInput`、`CreateSearchIndexInput`、`SearchIndexInput`、`GetKeySearchIndexesInput`、`KeySearchIndexSummary`、`SearchIndexAttribute`、`SearchIndexInfo`、`SearchQueryInput`、`SearchKeyResult` 和 `SearchQueryResult`，字段与已批准 spec 完全一致。

  `CreateSearchIndexInput::validate` 必须检查 connection id、index、每个 field/prefix 的 trim 非空和 256 字节限制、字段数量 1..=64、prefix 数量 <=64、字段名不重复；`SearchIndexInput` 检查 connection id 与 index；`GetKeySearchIndexesInput` 检查 connection id 与 key 512 字节限制；`SearchQueryInput` 检查 connection id、index、query、4096 字节限制和 offset/limit 范围。连接 id 为空返回 `InvalidConnection`，其他 Search 参数违规返回 `InvalidInput`。

  - [ ] **GREEN：扩展 capability 和错误。** 在 `ModuleCapabilities` 增加 `search_supported: bool` 与 `search_version: Option<String>`；`from_modules` 用 trim 后的 ASCII 小写识别 `search` 和 `redisearch` 的大小写变体，保留第一个非空版本。提供 `search_version_supported(Option<&str>)` 和 `ModuleCapabilities::search_compatible()`，按三段数字比较，缺段按 0，无法解析或低于 `2.0.0` 返回 false；保留现有 RedisJSON 行为。`AppError` 增加 `UnsupportedFeature`，`code()` 返回 `UNSUPPORTED_FEATURE`，`message()` 返回 `当前 Redis 功能不可用`。

- [ ] 更新 `src-tauri/src/domain/mod.rs` 的 module 声明和 re-export，并给现有 Rust 测试中所有 `ModuleCapabilities` 字面量补上 `search_supported`、`search_version`；给现有前端测试 fixture 预留相同字段，避免只因 DTO 扩展造成非 Search 回归。

- [ ] **REFACTOR/验证：**

  ```bash
  cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
  cargo test --manifest-path src-tauri/Cargo.toml --test domain search_ -- --nocapture
  git diff --check
  ```

- [ ] 提交：`增加 RedisSearch 领域协议`。

## Task 2: 实现 RESP2/RESP3 parser 和安全命令构造

**Files:** 新建 `src-tauri/src/redis/search.rs`；修改 `src-tauri/src/redis/mod.rs`；必要时只从 `src-tauri/src/redis/json_ops.rs` 提取可复用的 module value helper，不改变既有 JSON 语义。

- [ ] **RED：先新建 parser 测试模块。** 在 `src-tauri/src/redis/search.rs` 先写 `#[cfg(test)] mod tests`，引用尚未实现的函数：`parse_search_index_list`、`parse_search_index_info`、`parse_search_query`、`parse_max_search_results`、`build_create_search_index_command`。测试必须使用 `redis::Value` fixture，不访问网络：

  - `parse_search_index_list_accepts_resp2_resp3_and_attribute`：普通 `Array([BulkString("idx_a"), BulkString("idx_b")])`、`Set`、`Attribute(data=Array(...))` 都返回按原顺序的两个 `SearchIndexSummary`，结果截断到 500 个。
  - `parse_search_index_info_accepts_flat_map_and_attribute`：flat pair array、`Map`、Attribute 包装都能提取 `index_name`、`key_type`、`prefixes`、`attributes`、`num_docs`、`num_terms`、`num_records`、`total_index_memory_bytes`；未知字段被忽略。
  - `parse_search_index_info_rejects_missing_required_shape_and_caps_attributes`：缺 `index_name`/`key_type`、奇数 flat array、超过 256 个 attributes、单个字符串超过 256 字节均返回 `CommandFailed`。
  - `parse_search_query_accepts_total_and_keys_only`：`Array([Int(3), BulkString("doc:1"), BulkString("doc:2")])` 返回 total=3、两条 key；空结果 `Array([Int(0)])` 合法；多于 200 个键返回 `CommandFailed`。
  - `parse_max_search_results_handles_resp2_minus_one_and_map`：`["MAXSEARCHRESULTS", "100"]` 得 `Some(100)`，`-1` 得 `None`，Map 形态同样通过，非法数字得 `None`。
  - `create_search_index_command_keeps_each_input_as_an_argument`：HASH/JSON、prefix、TEXT/TAG/NUMERIC/GEO/GEOSHAPE/VECTOR 的命令 packed bytes 中每个参数均为独立 RESP bulk argument；字段名或 query 含空格时不能改变参数边界；不出现 `DD`。

- [ ] 运行 RED：

  ```bash
  cargo test --manifest-path src-tauri/Cargo.toml search::tests --lib -- --nocapture
  ```

  预期失败为 parser/helper 未定义或断言不满足。

- [ ] **GREEN：实现 `redis/search.rs`。** 提供以下 `pub(crate)` 函数和仅供模块内部使用的 helper：

  - `parse_search_index_list(Value) -> Result<Vec<SearchIndexSummary>, AppError>`：先展开 Attribute，再接受 Array/Set；每个元素转成受限字符串；忽略空索引名；最多保留 `MAX_SEARCH_INDEXES`。
  - `parse_search_index_info(Value) -> Result<SearchIndexInfo, AppError>`：递归展开 Attribute，支持 Map 和偶数长度 flat pair array；把 key 统一为小写比较；只读取声明字段；`prefixes` 最多 64，`attributes` 最多 256；attributes 支持 flat pair、Map、嵌套 Array/Map；`index_name`、`key_type`、schema identifier/type 缺失返回 `CommandFailed`；未知可选字段忽略。
  - `parse_search_query(Value, offset, limit) -> Result<SearchQueryResult, AppError>`：递归展开 Attribute；第一个整数为 total，其后每项为 key；key 数量不得超过 `min(limit, MAX_SEARCH_PAGE)` 和 200；`next_offset` 只有在 `offset + keys.len() < total` 时设置。
  - `parse_max_search_results(Value) -> Option<u64>`：只把正整数转换为 `Some`，`-1`、0、非法结构和非法数字返回 `None`，不把可选配置失败升级为 parser 错误。
  - `build_create_search_index_command(&CreateSearchIndexInput) -> Result<redis::Cmd, AppError>`：按 `FT.CREATE`, index, `ON`, key type, 可选 `PREFIX`, `SCHEMA` 顺序调用 `.arg()`；字段类型按 `TEXT`、`TAG`、`NUMERIC`、`GEO`、`GEOSHAPE`、`VECTOR` 映射；不接受未经 domain 校验的输入，不添加高级 schema 参数。
  - `redis_value_size(&Value) -> usize`：递归估算 String/Array/Map/Attribute 等 payload 大小，超过 `MAX_SEARCH_RESPONSE_BYTES` 时 parser 返回 `CommandFailed`。

- [ ] parser 不把 `redis::Value`、原始 bytes 或 Redis 错误文本写进 DTO；字符串只在 UTF-8 且长度受限时转换。将 `src-tauri/src/redis/mod.rs` 加入 `mod search;`，不把 parser 暴露为前端 API。

- [ ] **REFACTOR/验证：**

  ```bash
  cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
  cargo test --manifest-path src-tauri/Cargo.toml search::tests --lib -- --nocapture
  git diff --check
  ```

- [ ] 提交：`增加 RedisSearch RESP 解析`。

## Task 3: 接入 RedisService、Tauri commands 和 Redis Stack ignored 流程

**Files:** 修改 `src-tauri/src/redis/connection_manager.rs`、`src-tauri/src/commands/mod.rs`、`src-tauri/src/lib.rs`；新建 `src-tauri/src/commands/search.rs`；修改 `src-tauri/tests/commands.rs`、`src-tauri/tests/redis_integration.rs`。

- [ ] **RED：先写 command/service contract 测试。** 在 `src-tauri/tests/commands.rs` 增加对 `commands::search::{list_search_indexes, create_search_index, get_search_index, delete_search_index, search_keys, get_key_search_indexes}` 的编译引用和空连接输入测试；在 `src-tauri/tests/redis_integration.rs` 先增加 `redis_stack_search_flow_when_redis_stack_is_available` 的测试入口，引用尚未实现的 service 方法。空 connection id 必须预期 `InvalidConnection` 或 `InvalidInput`，不得建立连接。

- [ ] 运行 RED：

  ```bash
  cargo test --manifest-path src-tauri/Cargo.toml --test commands search -- --nocapture
  ```

  预期失败原因是 `RedisOperations` 尚无 Search 方法、commands 尚未注册。

- [ ] **GREEN：扩展 `RedisOperations`。** 增加精确签名：

  ```rust
  async fn list_search_indexes(
      &self,
      connection_id: &str,
  ) -> Result<ListSearchIndexesResult, AppError>;
  async fn create_search_index(
      &self,
      input: CreateSearchIndexInput,
  ) -> Result<(), AppError>;
  async fn get_search_index(
      &self,
      input: SearchIndexInput,
  ) -> Result<SearchIndexInfo, AppError>;
  async fn delete_search_index(
      &self,
      input: SearchIndexInput,
  ) -> Result<(), AppError>;
  async fn search_keys(
      &self,
      input: SearchQueryInput,
  ) -> Result<SearchQueryResult, AppError>;
  async fn get_key_search_indexes(
      &self,
      input: GetKeySearchIndexesInput,
  ) -> Result<Vec<KeySearchIndexSummary>, AppError>;
  ```

- [ ] 在 `RedisService` 增加私有 `ensure_search_supported(connection_id)`：调用已有 `get_module_capabilities`，当 `search_compatible()` 为 false 时返回 `UnsupportedFeature`；打开、关闭、切换数据库和替换 active client 继续复用已有 generation 与 capability 清理路径。

- [ ] 实现六条 service 流程：

  - `list_search_indexes` 校验 connection id，取得当前 active client，执行 `FT._LIST`，用 parser 解码，最多返回 500 个 `SearchIndexSummary`。
  - `create_search_index` 先 `input.validate()` 和 capability 检查，再调用 `build_create_search_index_command`；执行成功只返回 `()`。
  - `get_search_index` 先校验和 capability 检查，执行 `FT.INFO index`，用 parser 返回结构化 `SearchIndexInfo`。
  - `delete_search_index` 只执行 `FT.DROPINDEX index`，不传 `DD`，所以保留原始 Redis key；Redis 错误统一走 `map_command_error`。
  - `search_keys` 先读取 `FT.CONFIG GET MAXSEARCHRESULTS`；正数配置与 200 上限共同计算 `safe_limit`，可选配置读取失败按 None 处理；执行 `FT.SEARCH index query NOCONTENT LIMIT offset safe_limit`，query 始终作为独立 `.arg()`；parser 返回 total/key；对 key 列表构造单次 pipeline，每个 key 只放一个 `TYPE` 命令，按返回顺序补成 `SearchKeyResult`，pipeline 长度或响应超限返回 `CommandFailed`。
  - `get_key_search_indexes` 读取当前 key 的 `TYPE`；非 `hash`/`ReJSON-RL`/`ReJSON-RS`/`JSON` 直接返回空列表；取得最多 500 个索引名后以每批最多 32 个 `FT.INFO` 命令组成 pipeline，在内存中按 key type 和 prefix 匹配；单个 info parser 失败跳过该索引，匹配结果只返回 `KeySearchIndexSummary`，不返回完整 raw info。

- [ ] 在 `src-tauri/src/commands/search.rs` 建立六个 `#[tauri::command]` wrapper：列表接收 `connection_id: String`，其余接收对应 input，命令名稳定为 `list_search_indexes`、`create_search_index`、`get_search_index`、`delete_search_index`、`search_keys`、`get_key_search_indexes`；wrapper 只做 DTO 校验/调用 service，不把 Redis 错误文本写入返回值。

- [ ] 在 `commands/mod.rs` re-export Search commands，在 `lib.rs` 的 `tauri::generate_handler!` 中逐一注册六个 command；更新 command 测试的 `AppError` 和 `ModuleCapabilities` fixture。

- [ ] **GREEN：写可选 Redis Stack 流程。** 使用环境变量 `REDIX_TEST_REDIS_STACK_URL`；未配置时测试 `println!("skipped: REDIX_TEST_REDIS_STACK_URL 未配置")` 后返回，不把 skip 当通过。配置时建立唯一前缀如 `redix:search:test:<uuid>:`，创建 HASH 文档、创建 `idx_<uuid>`，验证列表、INFO、`FT.SEARCH` total/key/type、key-index 关联；删除 index 后确认文档仍存在，再删除本测试拥有的唯一 key。清理使用明确的唯一 key/index，不使用 `KEYS`，连接错误只让 ignored 流程失败。

- [ ] **REFACTOR/验证：**

  ```bash
  cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
  cargo test --manifest-path src-tauri/Cargo.toml --test commands search -- --nocapture
  cargo test --manifest-path src-tauri/Cargo.toml --test redis_integration redis_stack_search_flow_when_redis_stack_is_available -- --ignored --nocapture
  git diff --check
  ```

  若环境变量未配置，记录 `skipped`；若配置但服务不可达，记录为真实环境失败，不修改测试使其通过。

- [ ] 提交：`实现 RedisSearch Rust 服务与命令`。

## Task 4: 接入 TypeScript DTO、typed IPC 和纯状态模型

**Files:** 修改 `src/lib/types.ts`、`src/lib/tauri.ts`、`src/lib/tauri.test.ts`；新建 `src/features/search/searchState.ts`、`src/features/search/searchState.test.ts`。

- [ ] **RED：先写 bridge/state 测试。**

  - 在 `src/lib/tauri.test.ts` 增加 `listSearchIndexes`、`createSearchIndex`、`getSearchIndex`、`deleteSearchIndex`、`searchKeys`、`getKeySearchIndexes` 的 import 和测试，逐一断言 invoke 命令名、`{ connection_id }` 或 `{ input }` 参数形状；增加 `ModuleCapabilities` fixture 的 Search 字段。
  - 在 `searchState.test.ts` 测试 `searchCapabilityState` 的 `loading`、`ready`、`unsupported` 三态；版本 1.x、缺失版本和 Search 模块缺失均 unsupported；2.0.0 及以上 ready。
  - 测试 `searchErrorMessage` 将 `UNSUPPORTED_FEATURE`、`COMMAND_FAILED`、`CONNECTION_FAILED`、`IPC_ERROR` 映射为固定中文；未知错误使用传入 fallback。
  - 测试 `nextSearchOffset(total, offset, returned)` 在还有结果时返回下一 offset，没有更多结果时为 null；测试 `replaceSearchResults` 只接受相同 connection/index/request token，旧响应返回原状态。

- [ ] 运行 RED：

  ```bash
  npm run test:frontend -- src/lib/tauri.test.ts src/features/search/searchState.test.ts
  ```

  预期失败原因为 Search 类型、wrapper 和纯状态函数不存在。

- [ ] **GREEN：扩展 `src/lib/types.ts`。** 将 `Workspace` 增加字面量 `"search-query"`；`ModuleCapabilities` 增加 `search_supported: boolean`、`search_version: string | null`；加入与 Rust 完全对应的 `SearchKeyType`、`SearchFieldType`、`SearchIndexFieldInput`、`SearchIndexSummary`、`SearchIndexInfo`、`SearchIndexAttribute`、`CreateSearchIndexInput`、`SearchIndexInput`、`GetKeySearchIndexesInput`、`KeySearchIndexSummary`、`SearchQueryInput`、`SearchKeyResult`、`SearchQueryResult`、`ListSearchIndexesResult` 类型。枚举 union 使用小写字符串。

- [ ] **GREEN：扩展 `src/lib/tauri.ts`。** 从 `./types` 引入新增 DTO，并实现：

  ```ts
  export function listSearchIndexes(connectionId: string): Promise<ListSearchIndexesResult>;
  export function createSearchIndex(input: CreateSearchIndexInput): Promise<void>;
  export function getSearchIndex(input: SearchIndexInput): Promise<SearchIndexInfo>;
  export function deleteSearchIndex(input: SearchIndexInput): Promise<void>;
  export function searchKeys(input: SearchQueryInput): Promise<SearchQueryResult>;
  export function getKeySearchIndexes(input: GetKeySearchIndexesInput): Promise<KeySearchIndexSummary[]>;
  ```

  wrapper 分别调用六个 snake_case command，继续通过现有 `call<T>` 归一化 `{ code, message }`；不在 bridge 层拼 Redis 命令。

- [ ] **GREEN：实现 `searchState.ts`。** 提供 `REDISEARCH_MIN_VERSION = "2.0.0"`、版本比较、`searchCapabilityState`、`searchErrorMessage`、`nextSearchOffset`、`replaceSearchResults` 和 `resetSearchState`。`replaceSearchResults` 的 token 结构固定为 `{ connectionId, index, requestId }`，连接或 index 不一致时丢弃结果。

- [ ] **REFACTOR/验证：**

  ```bash
  npm run test:frontend -- src/lib/tauri.test.ts src/features/search/searchState.test.ts
  npm run build
  git diff --check
  ```

- [ ] 提交：`接入 RedisSearch typed IPC 状态`。

## Task 5: 增加 Search / Query React 工作区和导航入口

**Files:** 新建 `src/features/search/SearchPage.tsx`、`src/features/search/SearchPage.test.tsx`；修改 `src/App.tsx`、`src/styles.css`、`src/app.smoke.test.tsx`。

- [ ] **RED：先写页面测试。** 在 `search.test.tsx` 使用现有的 Vitest/Testing Library IPC mock，覆盖：

  - capability pending 时显示 `正在检测 RedisSearch…`，按钮不可提交；
  - Search 模块缺失或版本不满足时显示 `当前连接不支持 RedisSearch。`，不调用列表接口，普通返回 Browser 文案可见；
  - ready 状态加载索引列表，显示 index name，点击 index 调用 INFO 并展示 key type、prefix、attribute；刷新期间保留最近成功列表；
  - 创建表单支持 HASH/JSON、prefix 输入和 field rows；空 index/空 field 在页面阻止提交；成功后刷新列表并选择新 index；创建期间按钮 disabled；
  - 删除需要 `window.confirm` 为 true 才调用 `deleteSearchIndex`，成功后清除详情/查询结果；取消不调用；
  - 查询表单调用 `searchKeys`，显示 total、key、type 和下一页；空结果显示空状态；没有更多结果时下一页 disabled；查询期间不能重复提交；
  - 旧 query promise 在 index 切换后 resolve 时不能覆盖新 index 结果；unmount 后 resolve 不触发 state update。

- [ ] 在 `src/app.smoke.test.tsx` 增加 Search 导航 fixture/mocks，验证未连接时 `Search / Query` 与 Browser 一样 disabled；打开连接后可以点击该入口，并显示页面标题。所有现有 `getModuleCapabilitiesMock` 返回值加 Search 字段。

- [ ] 运行 RED：

  ```bash
  npm run test:frontend -- src/features/search/search.test.tsx src/app.smoke.test.tsx
  ```

  预期失败原因为页面、导航入口和相关 mock 尚不存在。

  - [ ] **GREEN：实现 `SearchPage.tsx`。** 页面只接收 `connectionId: string`；建立 capability、index list、selected index、info、query form、query result、error 和各 loading state。进入/切换 connection 时递增所有 request id，先加载 capability；只有 `searchCapabilityState` 为 ready 才加载列表。列表刷新失败保留最近成功数组，仅更新错误提示。

  页面结构固定包含：

  - 页面标题 `Search / Query`、刷新按钮；
  - 索引列表 `role="list"`，每项按钮名称为 index；
  - 创建 form：索引名称、key type select、prefix textarea（每行一个）、至少一个 field row（name + type select），提交按钮 `创建索引`；
  - 详情区显示 `索引信息`、key type、prefix 列表、attributes 表和统计值；
  - 删除按钮 `删除索引`；
  - 查询 form：索引 select、查询语句 input、执行按钮 `执行查询`、上一页/下一页；结果表显示 `total`、key 名称和 `key_type`。

  使用普通 React controls 和现有 button/field/table 样式；不新增持久化 Search history。查询文本只放在 `SearchQueryInput`，不自动执行、不写 Workbench history。

- [ ] **GREEN：修改 `App.tsx`。** 引入 `SearchQueryPage`，给 `NavigationItem["icon"]` 增加 `"search-query"`，导航增加 label `Search / Query`、description `索引与查询`、section description `管理 RedisSearch 索引并分页查询`；复用 code/query-library SVG 风格；在 active profile 且 active section 为 `search-query` 时渲染页面。未连接时依靠现有 `canAccessWorkspace` 自动禁用。

- [ ] **GREEN：修改样式。** 在 `src/styles.css` 增加 search workspace 的 index list、schema form、info grid、query result、unsupported/loading/error 状态样式，沿用现有 spacing、border、color variables；不引入新 UI 依赖。

- [ ] **REFACTOR/验证：**

  ```bash
  npm run test:frontend -- src/features/search/search.test.tsx src/app.smoke.test.tsx
  npm run build
  git diff --check
  ```

- [ ] 提交：`增加 RedisSearch 查询工作区`。

## Task 6: 在 Browser 键详情中显示关联 Search 索引

**Files:** 新建 `src/features/search/KeySearchIndexes.tsx`、`src/features/search/KeySearchIndexes.test.tsx`；修改 `src/features/browser/BrowserPage.tsx`、`src/features/browser/KeyDetails.tsx`、`src/features/browser/browserState.ts`、`src/features/browser/browser.test.tsx`。

- [ ] **RED：先写关联组件和 Browser 竞态测试。** 覆盖：

  - Search capability ready 且选中 HASH/JSON key 时调用 `getKeySearchIndexes({ connection_id: "local", key: "doc:1" })`，显示 `已关联索引` 和每个 index/prefix；
  - capability loading/failed/不兼容或非 HASH/JSON 时不调用接口，普通 key detail 仍完整显示；
  - connection 或 key 切换后旧 promise resolve，不得把旧 key 的 index badge 渲染到新 key；卸载后不更新状态；
  - Browser 初始 SCAN 与模块探测仍并行/互不阻塞，Search association 失败只显示局部错误，不覆盖 key detail 错误。

- [ ] 运行 RED：

  ```bash
  npm run test:frontend -- src/features/search/KeySearchIndexes.test.tsx src/features/browser/browser.test.tsx
  ```

  预期失败原因为 `getKeySearchIndexes` wrapper/component 尚不存在。

  - [ ] **GREEN：在 `KeyDetails.tsx` 中实现关联摘要。** 接收已有 `connectionId`、`detail.key`、`detail.key_type` 和 `moduleProbe`；使用 `searchCapabilityState` 判断可用性；以 `{ connectionId, key }` 和递增 request id 做响应身份校验；只保留 `KeySearchIndexSummary[]`，不展示 schema raw map。无匹配时显示 `未关联 RedisSearch 索引`，请求失败显示 `索引关联暂不可用。`。

- [ ] **GREEN：接入 `KeyDetails.tsx` 和 Browser。** 在 `KeyDetails` detail header/metadata 之后渲染 `KeySearchIndexes`，传入已有 `moduleProbe` 和当前 `detail.key/key_type`；不改变现有 JSON Path、Stream Group、rename/delete 生命周期。若 `detail` 为空不挂载关联请求。必要时仅扩展 `browserState.ts` 的错误文案，保持 module probe 的 Search 字段。

- [ ] 更新 `browser.test.tsx` 的 tauri mock、capability fixture 和新增调用断言；保持所有现有 Browser key CRUD、JSON、Stream、删除竞态测试通过。

- [ ] **REFACTOR/验证：**

  ```bash
  npm run test:frontend -- src/features/search/KeySearchIndexes.test.tsx src/features/browser/browser.test.tsx
  npm run build
  git diff --check
  ```

- [ ] 提交：`接入 Browser RedisSearch 索引关联`。

## Task 7: 更新非 Cloud 文档并完成全量验收

**Files:** 修改 `README.md`、`docs/non-cloud-scope.md`、`task_plan.md`、`findings.md`、`progress.md`；必要时修改测试 fixture，不修改参考项目 `/Users/ushopal/workspace/myself/RedisInsight`。

- [ ] 在 `README.md` 能力段加入本地 RedisSearch 说明：RedisSearch 可用时支持索引列表、HASH/JSON 索引创建/删除、INFO、`FT.SEARCH ... NOCONTENT LIMIT` 分页和 Browser 索引关联；模块缺失时局部降级；查询文本不持久化。

- [ ] 在 `docs/non-cloud-scope.md` 的允许项增加上述 Search 能力与 `MODULE LIST` 版本探测；从“明确排除项”中移除 `Search/Query`，保留 Vector、Array、SSH、Sentinel、Cluster、Cloud、AI、Telemetry、SQL 等未实现边界；在人工审查清单增加“Search 不使用 `KEYS`、SQL 或未约束 raw reply”。

- [ ] 在 `task_plan.md` 将 Phase 15 状态更新为 `implementation_in_progress` 或 `completed` 的实际状态，勾选本计划各阶段并追加本轮错误记录；在 `findings.md` 记录与 RedisInsight 对照的 RedisSearch 路由/命令证据、实现边界和未实现批次；在 `progress.md` 按实际输出记录 RED/GREEN、Redis Stack skip/通过、全量命令结果。任何失败必须写原因和下一步，不把未执行的网络流程写成通过。

- [ ] 运行最终验证矩阵：

  ```bash
  npm run test:frontend
  npm run build
  npm run check:non-cloud
  npm run test:rust
  cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
  git diff --check
  ```

- [ ] 若配置了 `REDIX_TEST_REDIS_STACK_URL`，再运行：

  ```bash
  cargo test --manifest-path src-tauri/Cargo.toml --test redis_integration redis_stack_search_flow_when_redis_stack_is_available -- --ignored --nocapture
  ```

  若未配置，明确记录 `skipped`。检查 `npm run check:non-cloud` 的扫描范围仅为 `src/`、`src-tauri/src/` 和 `package.json`，Search production code 不出现任何被排除的云入口词。

- [ ] 检查 staged diff：Search DTO、command、wrapper、页面、Browser 关联、文档均在范围内；没有参考项目变更、Cloud/SQL/拓扑文件、密码/证书输出或生产 `KEYS`。

- [ ] 提交最终验证记录：`完成 RedisSearch 第一批验证`。

## Completion Criteria

- [x] RedisSearch 可用时能在当前 Standalone 数据库完成索引列表、创建、INFO、删除保留原 key、有限分页查询和 Browser 键关联。
- [x] RedisSearch 缺失或版本不满足时 Search 页面显示稳定降级，普通 Browser、JSON、Workbench、Database 和 Observability 不被阻断。
- [x] Rust/TypeScript DTO、Tauri command、RESP2/RESP3 parser、输入限制、错误边界、旧响应丢弃、卸载清理和 loading 防重复提交都有定向测试。
- [x] 全量前端测试、生产构建、非 Cloud 扫描、Rust 测试、rustfmt 和 diff check 有本轮新鲜证据；Redis Stack 网络流程按真实配置记录通过或 skip。

## 本轮执行结果（2026-08-28）

- [x] Task 1–3：Rust domain、capability、RESP parser、命令构造、Redis service、Tauri commands。
- [x] Task 4–6：typed IPC、Search / Query 页面、索引 CRUD/详情/分页、Browser Hash/JSON 键关联。
- [x] Task 7：范围文档、计划记录、全量验证矩阵和 Redis Stack ignored skip 事实已同步。
