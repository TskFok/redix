# Redis Vector Set 与 Array 功能补全实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox（- [ ]）syntax for tracking.

**Goal:** 在当前 Redix 的 Standalone TCP/TLS Browser 中补齐 Redis Array 与 Vector Set 的能力探测、创建、浏览、编辑、删除和查询，并保持模块不可用时的局部降级。

**Architecture:** 沿用现有 Rust RedisService、Tauri typed IPC 和 React Browser 分层。Rust domain 层负责输入/输出合同和资源上限，专用 Array/Vector Set Redis 模块负责命令构造与 RESP2/RESP3 解析，RedisService 复用当前活跃连接，React 详情组件通过 typed bridge 执行分页读写和竞态保护。Array/Vector Set 不新增顶层 Workspace，而是在 Browser 的 key details 内按实际类型渲染。

**Tech Stack:** Rust 2021、Tauri 2、redis crate 1.5、Serde、Tokio、React 19、TypeScript 5.8、Vitest、Testing Library。

**Spec:** `docs/superpowers/specs/2026-08-28-vector-array-design.md`

## Global Constraints

- 只实现 Standalone TCP/TLS 活跃连接。
- 连接能力为 false 时，连接仍然成功，已有功能不受影响，新增模块操作返回结构化 UnsupportedFeature。
- 不通过模块名猜测能力；Array/Vector Set 能力必须由版本/命令能力探测得到。
- Array 索引在 Rust 中使用 u64，在 IPC/TypeScript 中使用规范十进制字符串，禁止用 JavaScript number 保存或回传索引。
- 所有前端可编辑数据都通过固定 DTO 和 typed IPC 传递，禁止把原始 redis::Value 暴露到前端，禁止新增任意 Redis 命令入口。
- RESP2/RESP3 parser 必须覆盖 bulk/simple string、integer/double、Array、nested Array、Map、Set、Attribute、Nil 和二进制 bulk string。
- 超出上限返回 InvalidInput，不自动扩大限制、截断写入或静默丢弃元素。
- 单次 Array 范围/扫描、搜索、聚合输入和批量写入最多 500 个元素。
- Array 文本元素最大 1 MiB，Array 索引字符串最大 20 字节。
- 单次 Vector Set 列表最多 200 个元素，VSIM top-k 最多 200，单次批量写入最多 200 个元素。
- Vector Set 单个属性最大 64 KiB，单个向量维度最多 4096，单个 FP32 向量传输最多 4 MiB，单个 IPC 响应 JSON 最多 4 MiB。
- 空列表、空范围和无匹配搜索是合法结果，不包装成错误。
- 任何遍历多个 key 的已有或新增逻辑都必须先批量拿到需要的 key/类型信息，再执行批量操作；禁止在循环中逐条查询 SQL。
- 不实现 Redis Cloud、Azure Managed Redis、RDI、Redis AI/Copilot、遥测、远程插件、SSH、Sentinel、Cluster、RedisJSON 深层树、Workbench 高级 CLI 和 SQL。
- 默认在当前 main 分支修改，不创建新分支；每个实现任务提交一条简体中文 commit。
- 每个任务遵循先写失败测试、确认 RED、实现最小代码、确认 GREEN、再提交的 TDD 顺序。

---

## 文件责任地图

### Rust domain 与协议

- Create: `src-tauri/src/domain/array.rs`：Array 输入、输出、枚举、校验和上限。
- Create: `src-tauri/src/domain/vector_set.rs`：Vector Set 输入、输出、向量/属性校验和上限。
- Modify: `src-tauri/src/domain/key.rs`：新 key 类型别名、RedisValue 摘要变体和类型规范化。
- Modify: `src-tauri/src/domain/module_capabilities.rs`：Array/Vector Set capability snapshot。
- Modify: `src-tauri/src/domain/mod.rs`：导出新 domain 类型。
- Modify: `src-tauri/src/error.rs`：补充 KeyNotFound 结构化错误及其序列化 code/message。
- Create: `src-tauri/src/redis/capabilities.rs`：COMMAND INFO 构造、RESP2/RESP3 命令存在性解析。
- Create: `src-tauri/src/redis/array.rs`：Array 命令构造、响应解析和 Redis-specific 兼容逻辑。
- Create: `src-tauri/src/redis/vector_set.rs`：Vector Set 命令构造、向量编码/解析、属性和 VSIM 兼容逻辑。
- Modify: `src-tauri/src/redis/mod.rs`：注册新 Redis 模块。
- Modify: `src-tauri/src/redis/connection_manager.rs`：能力探测、key 摘要、trait、service 方法和错误映射。
- Modify: `src-tauri/src/redis/database_analysis.rs`：为已发现的新 key 类型批量读取 ARLEN/VCARD。
- Modify: `src-tauri/Cargo.toml`、`src-tauri/Cargo.lock`：加入 FP32/base64 编解码依赖并锁定版本。

### Tauri IPC 与前端

- Create: `src-tauri/src/commands/array.rs`、`src-tauri/src/commands/vector_set.rs`：受控 Tauri command adapter。
- Modify: `src-tauri/src/commands/mod.rs`、`src-tauri/src/lib.rs`：注册并暴露新 commands。
- Modify: `src/lib/types.ts`：能力字段、RedisValue、Array/Vector Set DTO。
- Modify: `src/lib/tauri.ts`：typed IPC wrappers。
- Modify: `src/features/browser/browserState.ts`：新类型匹配、标签、克隆和错误提示。
- Create: `src/features/browser/arrayState.ts`：Array 详情分页、筛选、竞态状态 helper。
- Create: `src/features/browser/vectorSetState.ts`：Vector Set 列表、VSIM 和竞态状态 helper。
- Create: `src/features/browser/ArrayDetails.tsx`、`src/features/browser/VectorSetDetails.tsx`：专用 key detail UI。
- Modify: `src/features/browser/KeyDetails.tsx`：按 key 类型选择专用详情组件。
- Modify: `src/features/browser/AddKey.tsx`：Array/Vector Set 创建表单。
- Modify: `src/features/browser/KeyList.tsx`、`src/features/browser/BrowserPage.tsx`：类型过滤和 capability 传递。
- Modify: `src/styles.css`：新表格、表单、状态和窄屏布局。

### 测试与文档

- Modify: `src-tauri/tests/domain.rs`、`src-tauri/tests/commands.rs`：domain/IPC 合同测试。
- Modify: `src-tauri/tests/redis_integration.rs`：可选 Redis Stack Array/Vector Set 真实流程。
- Modify: `src/lib/tauri.test.ts`、`src/features/browser/browserState.test.ts`、`src/features/browser/browser.test.tsx`：typed bridge、状态和 Browser 回归。
- Create: `src/features/browser/ArrayDetails.test.tsx`、`src/features/browser/VectorSetDetails.test.tsx`：模块详情交互测试。
- Modify: `README.md`、`docs/non-cloud-scope.md`：同步非 Cloud 功能矩阵。

## Task 1: 建立 Array/Vector Set domain 合同和 capability 字段

**Files:**

- Create: `src-tauri/src/domain/array.rs`
- Create: `src-tauri/src/domain/vector_set.rs`
- Modify: `src-tauri/src/domain/key.rs`:30-41、219-287
- Modify: `src-tauri/src/domain/module_capabilities.rs`:17-78
- Modify: `src-tauri/src/domain/mod.rs`:1-25
- Modify: `src-tauri/src/error.rs`:5-65
- Test: `src-tauri/tests/domain.rs`:新增 Array/Vector Set 合同测试

**Interfaces:**

- Produces Array inputs `CreateArrayInput`、`ArrayKeyInput`、`ArrayRangeInput`、`ArrayScanInput`、`ArrayElementInput`、`ArrayMultiGetInput`、`SetArrayElementInput`、`AppendArrayInput`、`DeleteArrayElementsInput`、`DeleteArrayRangeInput`、`SearchArrayInput` 和 `AggregateArrayInput`。
- Produces Array outputs `ArraySummary`、`ArrayElement`、`ArrayCell`、`ArrayRange`、`ArrayScan`、`ArraySearchResult`、`ArrayAggregateResult` 和 `ArrayMutationResult`。
- Produces Vector Set inputs `CreateVectorSetInput`、`AddVectorSetElementsInput`、`ListVectorSetElementsInput`、`VectorSetElementPayload`、`VectorSetElementInput`、`SetVectorSetAttributesInput`、`DeleteVectorSetElementsInput` 和 `VectorSimilarityQueryInput`。
- Produces Vector Set outputs `VectorSetSummary`、`VectorSetElement`、`VectorSetPage`、`VectorSimilarityMatch` 和 `VectorSimilarityResult`。
- Extends `ModuleCapabilities` with `array_supported: bool` and `vector_set_supported: bool`。
- Extends `RedisValue` with summary-only `Array { length: String, count: String }` and `VectorSet { total: String, dimension: Option<u32>, quantization: Option<String> }`。

- [ ] **Step 1: 写 domain 失败测试**

在 `src-tauri/tests/domain.rs` 追加边界测试，先固定索引规范化、稀疏创建、Vector Set 三种输入互斥和 capability 字段：

```rust
#[test]
fn array_and_vector_domain_contracts_reject_precision_and_payload_errors() {
    let input = ArrayRangeInput {
        connection_id: "local".into(),
        key: "events".into(),
        start: "18446744073709551615".into(),
        end: "18446744073709551616".into(),
    };
    assert_eq!(input.validate().unwrap_err(), AppError::InvalidInput);

    let query = VectorSimilarityQueryInput {
        connection_id: "local".into(),
        key: "embeddings".into(),
        by_element: Some("seed".into()),
        by_vector: Some(vec![1.0]),
        by_vector_base64: None,
        count: 10,
        with_attributes: true,
    };
    assert_eq!(query.validate().unwrap_err(), AppError::InvalidInput);
}

#[test]
fn module_capabilities_require_new_command_sets() {
    let capabilities = ModuleCapabilities::from_modules_and_commands(
        vec![],
        ["ARLEN", "ARCOUNT", "ARSET"].into_iter().map(String::from).collect(),
    );
    assert!(!capabilities.array_supported);
    assert!(!capabilities.vector_set_supported);
}
```

- [ ] **Step 2: 运行失败测试确认 RED**

运行：

```bash
cargo test --manifest-path src-tauri/Cargo.toml domain::array_and_vector_domain_contracts_reject_precision_and_payload_errors -- --exact
cargo test --manifest-path src-tauri/Cargo.toml domain::module_capabilities_require_new_command_sets -- --exact
```

预期：由于新类型、字段和校验尚未存在，测试编译失败；不得跳过失败直接写生产实现。

- [ ] **Step 3: 实现最小 domain 合同**

在两个新 domain 文件中定义 serde DTO 和固定常量：

```rust
pub const MAX_ARRAY_ELEMENTS_PER_READ: usize = 500;
pub const MAX_ARRAY_BATCH_ELEMENTS: usize = 500;
pub const MAX_ARRAY_VALUE_BYTES: usize = 1024 * 1024;
pub const MAX_ARRAY_INDEX_BYTES: usize = 20;
pub const MAX_VECTOR_ELEMENTS_PER_PAGE: usize = 200;
pub const MAX_VECTOR_TOP_K: u32 = 200;
pub const MAX_VECTOR_BATCH_ELEMENTS: usize = 200;
pub const MAX_VECTOR_DIMENSION: usize = 4096;
pub const MAX_VECTOR_ATTRIBUTE_BYTES: usize = 64 * 1024;
pub const MAX_VECTOR_BINARY_BYTES: usize = 4 * 1024 * 1024;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ArrayCreateMode {
    Contiguous,
    Sparse,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct ArrayElement {
    pub index: String,
    pub value: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct ArrayCell {
    pub index: String,
    pub value: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct ArraySummary {
    pub key: String,
    pub length: String,
    pub count: String,
    pub next_index: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct ArrayMutationResult {
    pub affected: u64,
    pub key_exists: bool,
    pub next_index: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct VectorSetElementPayload {
    pub name: String,
    pub vector_values: Option<Vec<f64>>,
    pub vector_fp32_base64: Option<String>,
    pub attributes: Option<serde_json::Value>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct VectorSetElementInput {
    pub connection_id: String,
    pub key: String,
    pub element: String,
}
```

其余 DTO 字段必须固定为以下合同，避免 service、command 和 TypeScript 各自发明形状：

- ArrayCreateMode::{Contiguous, Sparse}；CreateArrayInput 为 connection_id、key、mode、start_index: Option<String>、values: Vec<String>、elements: Vec<ArrayElement>、ttl_ms: Option<i64>。
- ArrayKeyInput 为 connection_id、key；ArrayRangeInput 另有 start、end；ArrayScanInput 另有可选 start、end 和 limit: usize；ArrayElementInput 另有 index；ArrayMultiGetInput 另有 indices: Vec<String>。
- SetArrayElementInput 为 connection_id、key、index、value；AppendArrayInput 为 connection_id、key、values: Vec<String>；DeleteArrayElementsInput 为 connection_id、key、indices: Vec<String>；DeleteArrayRangeInput 另有 start、end。
- ArrayRange 为 cells: Vec<ArrayCell>、start、end、has_more；ArrayScan 为 elements: Vec<ArrayElement>、next_start: Option<String>、has_more。使用 cells 是为了让稀疏范围中的 Nil 位置可见且不丢失索引。
- SearchArrayInput 为 connection_id、key、可选 start/end、predicates: Vec<ArrayPredicate>、可选 combinator、nocase、with_values 和 limit；AggregateArrayInput 为 connection_id、key、operation、可选 start/end、values: Vec<String> 和 limit。
- ArraySearchResult 为 elements: Vec<ArrayElement>、total: String；ArrayAggregateResult 为 operation、value: String。
- CreateVectorSetInput 为 connection_id、key、dimension: u32、quantization: Option<String>、elements: Vec<VectorSetElementPayload>、ttl_ms: Option<i64>；AddVectorSetElementsInput 为 connection_id、key 和同样的 elements。
- ListVectorSetElementsInput 为 connection_id、key、可选 start/end 和 limit: usize；SetVectorSetAttributesInput 为 connection_id、key、element、attributes: serde_json::Value；DeleteVectorSetElementsInput 为 connection_id、key、elements: Vec<String>。
- VectorSetSummary 为 key、total: String、dimension: Option<u32>、quantization: Option<String>；VectorSetElement 为 name、score: Option<f64>、vector_base64: Option<String>、attributes: Option<serde_json::Value>；VectorSetPage 为 elements、cursor: Option<String>、has_more。
- VectorSimilarityQueryInput 为 connection_id、key、by_element: Option<String>、by_vector: Option<Vec<f64>>、by_vector_base64: Option<String>、count: u32 和 with_attributes；VectorSimilarityResult 为 matches: Vec<VectorSimilarityMatch>、has_more，其中 match 含 name、score: f64、attributes。

`CreateArrayInput::validate` 必须要求连接和 key 非空、TTL 大于等于 0；Contiguous 模式只接受非空 `values` 和一个可选规范化起始索引，Sparse 模式只接受非空且索引唯一的 `elements`。`VectorSetElementPayload::validate` 和 `VectorSimilarityQueryInput::validate` 必须强制三选一向量来源、所有数字有限、base64 解码后是非空 little-endian FP32、维度不超过 4096、属性 JSON UTF-8 字节数不超过 64 KiB；service 结合新建或已有 key 的 dimension 再拒绝维度不一致。`VectorSetElementInput::validate` 只校验 connection_id、key 和 element 非空。

`CreateVectorSetInput.elements` 和 `AddVectorSetElementsInput.elements` 使用 `Vec<VectorSetElementPayload>`；`VectorSetElementInput` 只用于已存在元素的读取、属性操作、删除和向量下载。

在 `key.rs` 中把 `array`、`vectorset`、`vector-set` 规范化为 `array` 或 `vector-set`，并让新 `RedisValue` 变体能 serde。`error.rs` 增加 `KeyNotFound`，用于专用详情 API 在 key/element 不存在时与命令错误区分。`ModuleCapabilities::from_modules` 保持已有行为，新增 `from_modules_and_commands` 只有在完整必需命令集合存在时才设置两个新 flag。

- [ ] **Step 4: 运行通过测试**

运行：

```bash
cargo test --manifest-path src-tauri/Cargo.toml domain::array_and_vector_domain_contracts_reject_precision_and_payload_errors -- --exact
cargo test --manifest-path src-tauri/Cargo.toml domain::module_capabilities_require_new_command_sets -- --exact
cargo test --manifest-path src-tauri/Cargo.toml domain
```

预期：新增合同测试和现有 domain 测试全部通过，现有 JSON/Search capability 字段行为不变。

- [ ] **Step 5: 提交**

```bash
git add src-tauri/src/domain/array.rs src-tauri/src/domain/vector_set.rs src-tauri/src/domain/key.rs src-tauri/src/domain/module_capabilities.rs src-tauri/src/domain/mod.rs src-tauri/src/error.rs src-tauri/tests/domain.rs
git commit -m "建立 Array 和 Vector Set 领域合同"
```

## Task 2: 接入命令能力探测和 key 摘要分派

**Files:**

- Create: `src-tauri/src/redis/capabilities.rs`
- Modify: `src-tauri/src/redis/mod.rs`:1-12
- Modify: `src-tauri/src/redis/json_ops.rs`:22-31（复用已有 MODULE LIST parser 或拆出调用）
- Modify: `src-tauri/src/redis/connection_manager.rs`:28-48、450-520、1201-1221、1297-1398、1420-1513
- Modify: `src-tauri/src/redis/database_analysis.rs`:189-221
- Test: `src-tauri/src/redis/capabilities.rs`、`src-tauri/src/redis/connection_manager.rs` 的单元测试

**Interfaces:**

- Produces `build_command_info_command() -> redis::Cmd`。
- Produces `parse_command_info(value: redis::Value) -> Result<HashSet<String>, AppError>`。
- Produces `is_array_command_set_supported(commands: &HashSet<String>) -> bool` 和 `is_vector_set_command_set_supported(commands: &HashSet<String>) -> bool`。
- `RedisService::get_module_capabilities` 先读取 MODULE LIST，再读取一次合并命令集合的 COMMAND INFO，并缓存完整 snapshot。
- `RedisService::get_key` 对 TYPE 返回 `array` 或 `vectorset` 时只读取轻量摘要；不读取完整模块数据。

- [ ] **Step 1: 写能力探测和摘要分派失败测试**

在新 capabilities 模块中先写 RESP2/RESP3 和缺失命令测试：

```rust
#[test]
fn command_info_parser_accepts_resp2_resp3_nil_and_attribute_entries() {
    let resp2 = Value::Array(vec![
        Value::Array(vec![text("ARLEN"), Value::Int(1)]),
        Value::Nil,
        Value::Array(vec![text("VADD"), Value::Int(1)]),
    ]);
    let parsed = parse_command_info(resp2).unwrap();
    assert!(parsed.contains("ARLEN"));
    assert!(parsed.contains("VADD"));
    assert_eq!(parsed.len(), 2);

    let resp3 = Value::Attribute {
        data: Box::new(Value::Set(vec![Value::Array(vec![
            text("VSIM"),
            Value::Int(1),
       ])])),
        attributes: vec![],
    };
    assert!(parse_command_info(resp3).unwrap().contains("VSIM"));
}

#[test]
fn key_type_normalization_includes_module_types() {
    assert_eq!(normalize_key_type("vectorset"), Some("vector-set"));
    assert_eq!(normalize_key_type("vector-set"), Some("vector-set"));
    assert_eq!(normalize_key_type("array"), Some("array"));
}
```

- [ ] **Step 2: 运行失败测试确认 RED**

运行：

```bash
cargo test --manifest-path src-tauri/Cargo.toml command_info_parser_accepts_resp2_resp3_nil_and_attribute_entries -- --exact
cargo test --manifest-path src-tauri/Cargo.toml key_type_normalization_includes_module_types -- --exact
```

预期：新 parser、命令集合和 key 类型分派尚不存在，测试编译失败或断言失败。

- [ ] **Step 3: 实现探测、缓存和摘要分派**

`capabilities.rs` 的必需命令集合固定为：

```rust
pub(crate) const ARRAY_COMMANDS: &[&str] =
    &["ARSET", "ARMSET", "ARGET", "ARMGET", "ARLEN", "ARCOUNT", "ARGETRANGE", "ARSCAN", "ARNEXT", "AROP", "ARGREP", "ARDEL", "ARDELRANGE", "ARINSERT", "ARRING", "ARINFO"];
pub(crate) const VECTOR_SET_COMMANDS: &[&str] =
    &["VADD", "VCARD", "VINFO", "VRANGE", "VRANDMEMBER", "VEMB", "VGETATTR", "VSETATTR", "VREM", "VSIM"];
```

`parse_command_info` 要递归解开 Attribute，接受 Array/Set/Map 中的命令条目，忽略 Nil，读取第一个字符串作为命令名，未知元数据字段不参与判断。`build_command_info_command` 只把上述固定命令名作为参数，不接受用户输入。Array/Vector Set 的 capability 只有在各自完整命令集合都存在时为 true；未安装模块、COMMAND INFO unknown command 或单个命令缺失均转为对应 false，不影响普通连接。

修改 `get_module_capabilities`：MODULE LIST 的网络、认证和协议失败沿用现有错误；COMMAND INFO 的网络、认证和协议错误映射为 `CommandFailed`，但不修改活跃连接状态，服务端 unknown command/模块未安装按空集合处理；单个命令缺失只产生 false flag。缓存写入继续经过现有 connection generation token，open/close/select database 的失效逻辑不变。

修改 `read_key`：Array 执行 ARLEN、ARCOUNT、ARNEXT 并构造 `RedisValue::Array`；Vector Set 执行 VCARD、VINFO 并构造 `RedisValue::VectorSet`。摘要结果和 PTTL 一起返回 `KeyValue`。`write_key` 对两个摘要变体返回 UnsupportedFeature，避免导入/通用保存误把摘要当完整数据写回。

修改 `key_size` 和 `database_analysis::length_command`：Array 使用 ARLEN，Vector Set 使用 VCARD；每次分析仍通过现有 pipeline 批量执行，不在 key 循环中单独查询。

- [ ] **Step 4: 运行通过测试**

运行：

```bash
cargo test --manifest-path src-tauri/Cargo.toml command_info_parser_accepts_resp2_resp3_nil_and_attribute_entries -- --exact
cargo test --manifest-path src-tauri/Cargo.toml key_type_normalization_includes_module_types -- --exact
cargo test --manifest-path src-tauri/Cargo.toml module_capabilities
cargo test --manifest-path src-tauri/Cargo.toml connection_manager
```

预期：探测 parser、旧 JSON/Search parser、connection manager 和 database analysis 现有测试通过；尚未连接支持模块时，普通 Redis 连接测试仍通过。

- [ ] **Step 5: 提交**

```bash
git add src-tauri/src/redis/capabilities.rs src-tauri/src/redis/mod.rs src-tauri/src/redis/json_ops.rs src-tauri/src/redis/connection_manager.rs src-tauri/src/redis/database_analysis.rs
git commit -m "接入模块命令能力探测和新类型摘要"
```

## Task 3: 实现 Array RESP parser 和命令构造

**Files:**

- Create: `src-tauri/src/redis/array.rs`
- Modify: `src-tauri/src/redis/mod.rs`:1-12
- Test: `src-tauri/src/redis/array.rs` 内部 parser/builder tests

**Interfaces:**

- Produces `parse_array_summary(key: &str, length: Value, count: Value, next_index: Value) -> Result<ArraySummary, AppError>`。
- Produces `parse_array_range(value: Value, start: &str, end: &str) -> Result<ArrayRange, AppError>`，空槽保留为 `ArrayCell { value: None }`。
- Produces `parse_array_scan(value: Value, limit: usize) -> Result<ArrayScan, AppError>`，只保留非空 index/value 对。
- Produces `parse_array_search(value: Value, limit: usize) -> Result<ArraySearchResult, AppError>`。
- Produces `parse_array_aggregate(value: Value, operation: &ArrayAggregateOperation) -> Result<ArrayAggregateResult, AppError>`。
- Produces固定参数构造器 `build_create_array_command`、`build_array_get_command`、`build_array_multi_get_command`、`build_array_range_command`、`build_array_scan_command`、`build_array_set_command`、`build_array_append_command`、`build_array_delete_command`、`build_array_search_command`、`build_array_aggregate_command`、`build_array_insert_command`、`build_array_ring_command` 和 `build_array_info_command`。

- [ ] **Step 1: 写 parser/builder 失败测试**

覆盖目标 RedisInsight 中已确认的 wire 形状和稀疏语义：

```rust
#[test]
fn parses_sparse_range_without_losing_indexes() {
    let reply = Value::Array(vec![
        Value::BulkString(b"first".to_vec()),
        Value::Nil,
        Value::BulkString(b"third".to_vec()),
    ]);
    let result = parse_array_range(reply, "10", "12").unwrap();
    assert_eq!(
        result.cells,
        vec![
            ArrayCell { index: "10".into(), value: Some("first".into()) },
            ArrayCell { index: "11".into(), value: None },
            ArrayCell { index: "12".into(), value: Some("third".into()) },
        ]
    );
}

#[test]
fn builds_array_search_without_command_string_interpolation() {
    let input = SearchArrayInput {
        connection_id: "local".into(),
        key: "arr".into(),
        start: None,
        end: None,
        predicates: vec![ArrayPredicate {
            criteria: "CONTAINS".into(),
            value: "a b".into(),
        }],
        combinator: None,
        nocase: true,
        with_values: true,
        limit: 50,
    };
    let command = build_array_search_command(&input).unwrap();
    assert_eq!(
        command.get_packed_command(),
        b"*10\r\n$6\r\nARGREP\r\n$3\r\narr\r\n$1\r\n-\r\n$1\r\n+\r\n$8\r\nCONTAINS\r\n$3\r\na b\r\n$6\r\nNOCASE\r\n$10\r\nWITHVALUES\r\n$5\r\nLIMIT\r\n$2\r\n50\r\n".to_vec()
    );
}
```

- [ ] **Step 2: 运行失败测试确认 RED**

运行：

```bash
cargo test --manifest-path src-tauri/Cargo.toml redis::array::parses_sparse_range_without_losing_indexes -- --exact
cargo test --manifest-path src-tauri/Cargo.toml redis::array::builds_array_search_without_command_string_interpolation -- --exact
```

预期：Array parser/builder 尚未定义，测试编译失败。

- [ ] **Step 3: 实现 parser 和 builder**

所有 builder 使用 `redis::Cmd.arg` 逐个添加参数，不拼接命令文本。parser 要递归解开 Attribute、兼容 flat pair 和 nested pair 两种 ARSCAN 结果；range 按请求方向保留 index 顺序，scan 丢弃 Nil 但保留真实 index；数字回复通过精确十进制字符串解析。

`ARSET`/`ARMSET`、`ARGET`/`ARMGET`、`ARINSERT`、`ARRING` 和 `ARINFO` 的 builder 也必须固定参数顺序并有单元测试，即使本批次 UI 只使用其中的读写子集。`ARGREP` 的参数顺序固定为 key、start（缺省 `-`）、end（缺省 `+`）、criteria/value 对、可选 AND/OR、NOCASE、WITHVALUES、LIMIT。`AROP` 只有 MATCH 操作追加 match value。解析器对必需字段缺失、奇数长度 pair、非 UTF-8 值、超出 500 项和超过 4 MiB 响应返回 CommandFailed/InvalidInput。

为 Array 追加一个精确的 `parse_array_index` helper：只接受 ASCII 十进制、无正号/负号、规范化后长度不超过 20 字节且不超过 u64::MAX；所有 domain 的 index 校验复用该 helper，防止 Rust 和前端各自实现不同规则。

- [ ] **Step 4: 运行通过测试**

运行：

```bash
cargo test --manifest-path src-tauri/Cargo.toml redis::array
cargo test --manifest-path src-tauri/Cargo.toml domain
```

预期：Array 全部 parser/builder 测试通过，现有 domain 测试无回归。

- [ ] **Step 5: 提交**

```bash
git add src-tauri/src/redis/array.rs src-tauri/src/redis/mod.rs
git commit -m "实现 Redis Array 协议解析和命令构造"
```

## Task 4: 实现 Array RedisService、Tauri commands 和 typed bridge

**Files:**

- Modify: `src-tauri/src/redis/connection_manager.rs`:51-115、319-760
- Create: `src-tauri/src/commands/array.rs`
- Modify: `src-tauri/src/commands/mod.rs`:1-35
- Modify: `src-tauri/src/lib.rs`:54-116
- Modify: `src/lib/types.ts`:1-90、597-630
- Modify: `src/lib/tauri.ts`:1-60、287-360
- Test: `src-tauri/tests/commands.rs`、`src/lib/tauri.test.ts`

**Interfaces:**

RedisOperations 增加以下精确方法：

```rust
async fn create_array(&self, input: CreateArrayInput) -> Result<KeyValue, AppError>;
async fn get_array_summary(&self, input: ArrayKeyInput) -> Result<ArraySummary, AppError>;
async fn get_array_range(&self, input: ArrayRangeInput) -> Result<ArrayRange, AppError>;
async fn scan_array(&self, input: ArrayScanInput) -> Result<ArrayScan, AppError>;
async fn get_array_elements(&self, input: ArrayMultiGetInput) -> Result<Vec<Option<String>>, AppError>;
async fn set_array_element(&self, input: SetArrayElementInput) -> Result<ArrayMutationResult, AppError>;
async fn append_array_elements(&self, input: AppendArrayInput) -> Result<ArrayMutationResult, AppError>;
async fn delete_array_elements(&self, input: DeleteArrayElementsInput) -> Result<ArrayMutationResult, AppError>;
async fn delete_array_range(&self, input: DeleteArrayRangeInput) -> Result<ArrayMutationResult, AppError>;
async fn search_array(&self, input: SearchArrayInput) -> Result<ArraySearchResult, AppError>;
async fn aggregate_array(&self, input: AggregateArrayInput) -> Result<ArrayAggregateResult, AppError>;
```

Tauri command 名称与 service 方法一致，所有 command 接收 `input` 对象。TypeScript wrapper 名称为 `createArray`、`getArraySummary`、`getArrayRange`、`scanArray`、`getArrayElements`、`setArrayElement`、`appendArrayElements`、`deleteArrayElements`、`deleteArrayRange`、`searchArray` 和 `aggregateArray`。

- [ ] **Step 1: 写 Array service/IPC 失败测试**

在 `src-tauri/tests/commands.rs` 增加 command adapter 的注册和输入校验测试，在 `src/lib/tauri.test.ts` 增加 wrapper 参数合同：

```typescript
it("把 Array 查询作为 typed input 传给 Tauri", async () => {
  const input = {
    connection_id: "local",
    key: "events",
    start: "0",
    end: "99",
  };
  await expect(getArrayRange(input)).resolves.toEqual({
    cells: [],
    start: "0",
    end: "99",
    has_more: false,
  });
  expect(invokeMock).toHaveBeenLastCalledWith("get_array_range", { input });
});
```

Rust command 测试先引用 `array::create_array`、`array::get_array_range` 和其余 adapters；此时应因 command 不存在而 RED。

- [ ] **Step 2: 运行失败测试确认 RED**

运行：

```bash
cargo test --manifest-path src-tauri/Cargo.toml exposes_all_tauri_command_adapters -- --exact
npm test -- --run src/lib/tauri.test.ts
```

预期：Rust 新 command 引用或 TypeScript wrapper 尚不存在，测试失败。

- [ ] **Step 3: 实现 Array service 和 IPC**

在 `RedisService` 中增加 `ensure_array_supported`，只检查 capability snapshot 的 `array_supported`。每个 public service 方法先执行 domain `validate`，再取当前 multiplexed connection；key 必须通过 `TYPE`/现有 key 检查确认，不存在返回 KeyNotFound，模块未安装返回 UnsupportedFeature，类型不匹配返回 UnsupportedDataType。

创建流程使用一次固定的 Array create command；若 `ttl_ms` 非空，使用现有 `apply_ttl`，最终调用摘要读取返回 `KeyValue`。批量写入使用一个 ARMSET 或单个 ARSET，不在循环中发起逐元素查询。删除元素使用一次 ARDEL，删除区间使用一次 ARDELRANGE。append 先读取 ARLEN 再用规范化下一个索引执行 ARSET，并将并发覆盖风险记录为 UI 可重试的 CommandFailed。

每个 Tauri command 只做 `state.redis.method(input).await` 转发；在 `commands/mod.rs` 和 `lib.rs` 完成注册。`src/lib/types.ts` 增加所有 domain DTO 的 snake_case 字段，`RedisValue` 的新摘要变体只用于 `KeyValue` 读取，`createArray` 不复用通用 `createKey`。

- [ ] **Step 4: 运行通过测试**

运行：

```bash
cargo test --manifest-path src-tauri/Cargo.toml exposes_all_tauri_command_adapters -- --exact
cargo test --manifest-path src-tauri/Cargo.toml commands
npm test -- --run src/lib/tauri.test.ts
npm run build
```

预期：Array command 注册、typed wrapper 参数和 TypeScript 编译全部通过。

- [ ] **Step 5: 提交**

```bash
git add src-tauri/src/redis/connection_manager.rs src-tauri/src/commands/array.rs src-tauri/src/commands/mod.rs src-tauri/src/lib.rs src-tauri/tests/commands.rs src/lib/types.ts src/lib/tauri.ts src/lib/tauri.test.ts
git commit -m "接入 Redis Array 服务和 typed IPC"
```

## Task 5: 实现 Vector Set parser、FP32 编解码和命令构造

**Files:**

- Modify: `src-tauri/Cargo.toml`、`src-tauri/Cargo.lock`
- Create: `src-tauri/src/redis/vector_set.rs`
- Modify: `src-tauri/src/redis/mod.rs`:1-12
- Test: `src-tauri/src/redis/vector_set.rs` 内部 parser/builder tests

**Interfaces:**

- Produces `encode_fp32_base64(values: &[f64]) -> Result<String, AppError>`。
- Produces `decode_fp32_base64(value: &str) -> Result<Vec<f32>, AppError>`。
- Produces `parse_vector_set_info(value: Value) -> Result<(Option<u32>, Option<String>), AppError>`。
- Produces `parse_vector_set_page(value: Value, limit: usize) -> Result<Vec<String>, AppError>`。
- Produces `parse_vector_set_element(vector: Value, attributes: Value, name: &str) -> Result<VectorSetElement, AppError>`。
- Produces `parse_vsim_reply(value: Value, with_attributes: bool) -> Result<Vec<VectorSimilarityMatch>, AppError>`。
- Produces `build_vadd_command`、`build_vrange_command`、`build_vsim_command`、`build_vsetattr_command`、`build_vrem_command`。

- [ ] **Step 1: 写 Vector Set parser/builder 失败测试**

```rust
#[test]
fn fp32_round_trip_uses_little_endian_and_rejects_non_finite_values() {
    let encoded = encode_fp32_base64(&[1.0, -2.5]).unwrap();
    assert_eq!(decode_fp32_base64(&encoded).unwrap(), vec![1.0, -2.5]);
    assert_eq!(
        encode_fp32_base64(&[f64::NAN]).unwrap_err(),
        AppError::InvalidInput
    );
}

#[test]
fn parses_flat_vsim_matches_with_optional_attributes() {
    let reply = Value::Array(vec![
        text("one"),
        Value::Double(0.95),
        text(r#"{"city":"shanghai"}"#),
        text("two"),
        Value::Double(0.81),
        Value::Nil,
    ]);
    let matches = parse_vsim_reply(reply, true).unwrap();
    assert_eq!(matches[0].name, "one");
    assert_eq!(matches[0].attributes, Some(serde_json::json!({"city":"shanghai"})));
    assert_eq!(matches[1].attributes, None);
}
```

- [ ] **Step 2: 运行失败测试确认 RED**

运行：

```bash
cargo test --manifest-path src-tauri/Cargo.toml redis::vector_set::fp32_round_trip_uses_little_endian_and_rejects_non_finite_values -- --exact
cargo test --manifest-path src-tauri/Cargo.toml redis::vector_set::parses_flat_vsim_matches_with_optional_attributes -- --exact
```

预期：base64 依赖、parser 和 builder 尚不存在，测试失败。

- [ ] **Step 3: 实现二进制/RESP parser 和 builder**

在 Cargo 中加入 `base64 = "0.22"`，使用 `base64::engine::general_purpose::STANDARD`；每个 f32 使用 `to_le_bytes`，解码时要求字节数是 4 的倍数且总大小不超过 4 MiB。

`VADD` builder 支持 VALUES 和 FP32 两种输入，参数分别为 key、格式、维度/字节 payload、element name，并在属性存在时追加单独的 VSETATTR 命令。`VRANGE` builder 使用 start、end、limit；`VSIM` builder 只接受 element、VALUES 或 FP32 之一，追加 COUNT、WITHSCORES，并根据参数决定 WITHATTRIBS。

`parse_vector_set_info` 兼容 RESP2 flat key/value、RESP3 Map 和 Attribute；`parse_vector_set_page` 兼容 name flat list 和 nested list，严格限制 200 项；`parse_vsim_reply` 支持 name/score 对和 name/score/attributes 三元组，属性字符串解析为 JSON，空属性保留 None，响应超过 4 MiB 返回 CommandFailed。

- [ ] **Step 4: 运行通过测试**

运行：

```bash
cargo test --manifest-path src-tauri/Cargo.toml redis::vector_set
cargo test --manifest-path src-tauri/Cargo.toml domain
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
```

预期：Vector Set parser/builder、FP32 边界和现有 Rust 格式检查通过。

- [ ] **Step 5: 提交**

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/redis/vector_set.rs src-tauri/src/redis/mod.rs
git commit -m "实现 Vector Set 协议解析和 FP32 编解码"
```

## Task 6: 实现 Vector Set RedisService、Tauri commands 和 typed bridge

**Files:**

- Modify: `src-tauri/src/redis/connection_manager.rs`:51-115、319-760
- Create: `src-tauri/src/commands/vector_set.rs`
- Modify: `src-tauri/src/commands/mod.rs`:1-35
- Modify: `src-tauri/src/lib.rs`:54-116
- Modify: `src/lib/types.ts`:1-90、597-630
- Modify: `src/lib/tauri.ts`:1-60、287-390
- Test: `src-tauri/tests/commands.rs`、`src/lib/tauri.test.ts`

**Interfaces:**

RedisOperations 增加：

```rust
async fn create_vector_set(&self, input: CreateVectorSetInput) -> Result<KeyValue, AppError>;
async fn add_vector_set_elements(&self, input: AddVectorSetElementsInput) -> Result<(), AppError>;
async fn get_vector_set_summary(&self, input: VectorSetKeyInput) -> Result<VectorSetSummary, AppError>;
async fn list_vector_set_elements(&self, input: ListVectorSetElementsInput) -> Result<VectorSetPage, AppError>;
async fn get_vector_set_element(&self, input: VectorSetElementInput) -> Result<VectorSetElement, AppError>;
async fn set_vector_set_attributes(&self, input: SetVectorSetAttributesInput) -> Result<VectorSetElement, AppError>;
async fn delete_vector_set_attributes(&self, input: VectorSetElementInput) -> Result<(), AppError>;
async fn delete_vector_set_elements(&self, input: DeleteVectorSetElementsInput) -> Result<u64, AppError>;
async fn search_vector_set(&self, input: VectorSimilarityQueryInput) -> Result<VectorSimilarityResult, AppError>;
async fn download_vector_embedding(&self, input: VectorSetElementInput) -> Result<String, AppError>;
```

TypeScript wrappers分别为 `createVectorSet`、`addVectorSetElements`、`getVectorSetSummary`、`listVectorSetElements`、`getVectorSetElement`、`setVectorSetAttributes`、`deleteVectorSetAttributes`、`deleteVectorSetElements`、`searchVectorSet` 和 `downloadVectorEmbedding`。

- [ ] **Step 1: 写 Vector Set service/IPC 失败测试**

```typescript
it("为 VSIM 保留三种输入之一和属性开关", async () => {
  const input = {
    connection_id: "local",
    key: "embeddings",
    by_element: "seed",
    by_vector: null,
    by_vector_base64: null,
    count: 10,
    with_attributes: true,
  };
  await expect(searchVectorSet(input)).resolves.toEqual({
    matches: [],
    has_more: false,
  });
  expect(invokeMock).toHaveBeenLastCalledWith("search_vector_set", { input });
});
```

在 `src-tauri/tests/commands.rs` 先加入全部 Vector Set command adapter 的函数引用和 `lib.rs` handler 预期；新 adapter 尚不存在时测试必须 RED。

- [ ] **Step 2: 运行失败测试确认 RED**

运行：

```bash
cargo test --manifest-path src-tauri/Cargo.toml exposes_all_tauri_command_adapters -- --exact
npm test -- --run src/lib/tauri.test.ts
```

预期：Vector Set command 和 typed wrapper 尚不存在，测试失败。

- [ ] **Step 3: 实现 Vector Set service 和兼容分支**

`create_vector_set` 先验证 key 不存在，再以一个 pipeline 执行最多 200 个 VADD/VSETATTR 命令和可选 PEXPIRE；`add_vector_set_elements` 对已存在 key 使用同样的批量 pipeline。属性 JSON 在进入 command 前以紧凑 JSON 编码，读取后再解析为 JSON value。

`list_vector_set_elements` 优先执行 VRANGE；仅当 RedisError detail 明确包含 unknown command 时回退 VRANDMEMBER，空结果与命令不可用区分处理。`get_vector_set_element` 使用一个 pipeline 同时执行 VEMB 和 VGETATTR。

`search_vector_set` 先构造带 WITHATTRIBS 的 VSIM；若 RedisError detail 明确表示 WITHATTRIBS 不受支持，则重试不带 WITHATTRIBS 的 VSIM，再用一个 VGETATTR pipeline 回填属性。任何权限、WRONGTYPE、非法参数和连接错误不触发静默回退。每个最终回复均经过 200 top-k 和 4 MiB 限制。

`download_vector_embedding` 返回 base64 FP32 字符串，不返回原始 Redis binary；不存在的 key/element 返回 KeyNotFound，空向量返回 InvalidInput。所有 public 方法先 capability gate 和 domain validate。

- [ ] **Step 4: 运行通过测试**

运行：

```bash
cargo test --manifest-path src-tauri/Cargo.toml exposes_all_tauri_command_adapters -- --exact
cargo test --manifest-path src-tauri/Cargo.toml commands
npm test -- --run src/lib/tauri.test.ts
npm run build
```

预期：Vector Set adapter、service trait 实现、typed wrappers 和全量 TypeScript 类型检查通过。

- [ ] **Step 5: 提交**

```bash
git add src-tauri/src/redis/connection_manager.rs src-tauri/src/commands/vector_set.rs src-tauri/src/commands/mod.rs src-tauri/src/lib.rs src-tauri/tests/commands.rs src/lib/types.ts src/lib/tauri.ts src/lib/tauri.test.ts
git commit -m "接入 Vector Set 服务和 typed IPC"
```

## Task 7: 接入 Browser key details、创建表单和竞态保护

**Files:**

- Create: `src/features/browser/arrayState.ts`
- Create: `src/features/browser/vectorSetState.ts`
- Create: `src/features/browser/ArrayDetails.tsx`
- Create: `src/features/browser/VectorSetDetails.tsx`
- Create: `src/features/browser/ArrayDetails.test.tsx`
- Create: `src/features/browser/VectorSetDetails.test.tsx`
- Modify: `src/features/browser/browserState.ts`:46-188、239-262
- Modify: `src/features/browser/KeyDetails.tsx`:424-577
- Modify: `src/features/browser/AddKey.tsx`:1-250
- Modify: `src/features/browser/KeyList.tsx`:62-80
- Modify: `src/features/browser/BrowserPage.tsx`:25-162、350-440
- Modify: `src/features/browser/browserState.test.ts`、`src/features/browser/browser.test.tsx`
- Modify: `src/styles.css`:1134-1410、1737-1810、2975-3255

**Interfaces:**

- `arrayState.ts` 输出 `ArrayDetailState`、`createArrayDetailState`、`applyArrayRange`、`applyArrayScan` 和 `isCurrentArrayRequest`。
- `vectorSetState.ts` 输出 `VectorSetDetailState`、`createVectorSetDetailState`、`applyVectorSetPage`、`applySimilarityResult` 和 `isCurrentVectorRequest`。
- `ArrayDetails` 接收 `{ connectionId, key, ttlMs, supported, busy, onBusyChange, onDetailChange, onDeleted }`。
- `VectorSetDetails` 接收同一组父级操作属性；所有按钮通过 typed bridge 调用，不直接使用 invoke。
- `KeyDetails` 只对 String/Hash/List/Set/Sorted Set/JSON/Stream 渲染 `KeyEditor`；Array 和 Vector Set 渲染专用详情组件。

- [ ] **Step 1: 写 Browser state/UI 失败测试**

在 `browserState.test.ts` 先固定类型标签和旧响应丢弃：

```typescript
it("识别 Array/Vector Set 标签并保留稀疏索引", () => {
  expect(keyTypeLabel("array")).toBe("Array");
  expect(keyTypeLabel("vectorset")).toBe("Vector Set");
  const next = applyArrayScan(
    createArrayDetailState("arr"),
    {
      elements: [{ index: "18446744073709551614", value: "x" }],
      next_start: null,
      has_more: false,
    },
    { requestId: 1, connectionId: "local", key: "arr" },
  );
  expect(next.elements[0].index).toBe("18446744073709551614");
});
```

在 `ArrayDetails.test.tsx` 和 `VectorSetDetails.test.tsx` 先写失败交互：Array range 显示空槽、保存单元格调用 set command；Vector Set 三种 VSIM source 互斥、查询调用 search command。模块不支持时按钮不可用并显示明确状态。

- [ ] **Step 2: 运行失败测试确认 RED**

运行：

```bash
npm test -- --run src/features/browser/browserState.test.ts
npm test -- --run src/features/browser/ArrayDetails.test.tsx
npm test -- --run src/features/browser/VectorSetDetails.test.tsx
```

预期：新 state helper、组件和类型标签尚不存在，测试失败。

- [ ] **Step 3: 实现 Browser state、组件和创建入口**

`browserState.ts` 增加 Array/Vector Set 的 `matchesKeyType`、`cloneRedisValue`、`redisValueKind` 和 `keyTypeLabel` 分支，并将 `UNSUPPORTED_FEATURE` 映射为“当前 Redis 不支持此模块功能”。更新所有测试 fixture 的 ModuleCapabilities，补上两个新 boolean 字段。

`ArrayDetails` 初次挂载读取 summary 和首个 range；提供 Range/Scan 切换、start/end、limit、搜索、聚合、单元格设置、追加、按索引删除和按区间删除。选中多行或展开详情时用一次 `getArrayElements` 批量读取，不按行发起请求。range 以 `ArrayCell.value === null` 显示空槽，scan 只显示 populated element，但两者都显示字符串 index。每次 mutation 完成后重新读取 summary 和当前视图；若返回 `key_exists: false` 调用 `onDeleted`。

`VectorSetDetails` 初次挂载读取 summary 和第一页元素；列表分页优先使用 cursor，不把 element name 当作 number；单元素详情显示向量 base64/解码后的有限数字和 JSON 属性。添加/更新、属性保存、属性删除、元素删除和下载向量均在 mutation 完成后重新读取当前页。

VSIM 表单提供 element name、numeric vector、FP32 base64 三个输入区，但提交前只允许一个非空；top-k 限制 1-200；查询结果显示 score 和属性。旧请求防护使用 connectionId、key、递增 requestId 三元组，响应在三者均匹配时才写入 state；组件卸载时递增 requestId。

`AddKey` 增加 array/vector-set 选项，但创建分别调用 `createArray`/`createVectorSet`。Array 表单支持 contiguous 起始 index + 每行 value 和 sparse 每行 index=value；Vector Set 表单支持 JSON 元素数组，每个元素必须提供 name 以及 numeric values 或 fp32_base64 之一。成功后回调父级刷新列表，切换连接时丢弃未完成请求。

`KeyList` 增加类型过滤选项；`BrowserPage` 将 moduleProbe 传给 `KeyDetails`，不因为 capability probe 失败而阻塞 SCAN。`styles.css` 增加 module detail table、range toolbar、similarity results、属性 JSON 和窄屏堆叠规则，不改变已有通用编辑器布局。

- [ ] **Step 4: 运行通过测试**

运行：

```bash
npm test -- --run src/features/browser/browserState.test.ts
npm test -- --run src/features/browser/ArrayDetails.test.tsx
npm test -- --run src/features/browser/VectorSetDetails.test.tsx
npm test -- --run src/features/browser/browser.test.tsx
npm run build
```

预期：新详情交互、旧响应丢弃、模块不可用降级和已有 Browser/JSON/Stream 测试全部通过。

- [ ] **Step 5: 提交**

```bash
git add src/features/browser/arrayState.ts src/features/browser/vectorSetState.ts src/features/browser/ArrayDetails.tsx src/features/browser/VectorSetDetails.tsx src/features/browser/ArrayDetails.test.tsx src/features/browser/VectorSetDetails.test.tsx src/features/browser/browserState.ts src/features/browser/KeyDetails.tsx src/features/browser/AddKey.tsx src/features/browser/KeyList.tsx src/features/browser/BrowserPage.tsx src/features/browser/browserState.test.ts src/features/browser/browser.test.tsx src/styles.css
git commit -m "接入 Array 和 Vector Set Browser 详情"
```

## Task 8: Redis Stack 集成测试、文档和全量验证

**Files:**

- Modify: `src-tauri/tests/redis_integration.rs`:1-360
- Modify: `README.md`
- Modify: `docs/non-cloud-scope.md`
- Modify: `task_plan.md`、`findings.md`、`progress.md`（仅更新本批次状态；保留并审查已有无关修改，不整文件覆盖）

**Interfaces:**

- Produces one ignored integration flow named `redis_stack_array_and_vector_set_flow_when_redis_stack_is_available`，环境变量继续使用 `REDIX_TEST_REDIS_STACK_URL`。
- Produces README/non-cloud scope entries that list Array and Vector Set as local supported modules and keep Cloud/AI/Telemetry/remote plugin/SQL exclusions explicit.
- Produces verification evidence with separate ordinary Redis, module-supported Redis and missing-module cases.

- [ ] **Step 1: 写集成和文档验证入口**

在 `redis_integration.rs` 加入一个真实的 ignored smoke flow，而不是空测试占位：能力缺失时只 skip 对应模块并保持另一个模块和普通 Redis 流程可继续；支持时使用带进程号和时间戳的唯一 key，创建、读取并删除最小 Array/Vector Set 数据，flow 与 cleanup 分开报告。扩展测试文件顶部的 domain import，复用现有 `integration_profile`、`TestSecrets` 和 `RedisService::new` 初始化方式。先在文档矩阵中把 Array/Vector Set 状态改为“实现中”，让非 Cloud 文档检查暴露需要同步的条目。

```rust
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "设置 REDIX_TEST_REDIS_STACK_URL 后用 cargo test -- --ignored --nocapture 运行"]
async fn redis_stack_array_and_vector_set_flow_when_redis_stack_is_available() {
    let Ok(url) = std::env::var("REDIX_TEST_REDIS_STACK_URL") else {
        eprintln!("skipped: REDIX_TEST_REDIS_STACK_URL is not set");
        return;
    };
    let (profile, password) = integration_profile(&url);
    let secrets = TestSecrets::default();
    if let Some(password) = password.as_deref() {
        secrets
            .write(
                "integration",
                &ConnectionSecrets {
                    password: Some(password.to_owned()),
                    ..ConnectionSecrets::default()
                },
            )
            .unwrap();
    }
    let service = RedisService::new(
        std::sync::Arc::new(TestProfiles {
            profiles: vec![profile],
        }),
        std::sync::Arc::new(secrets),
    );
    service.open_connection("integration").await.unwrap();
    let suffix = format!(
        "{}:{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock must be after Unix epoch")
            .as_nanos()
    );
    let array_key = format!("redix:array:integration:{suffix}");
    let vector_key = format!("redix:vector-set:integration:{suffix}");

    let flow = async {
        let capabilities = service
            .get_module_capabilities("integration")
            .await
            .map_err(|error| error.code().to_owned())?;
        if capabilities.array_supported {
            let created = service
                .create_array(CreateArrayInput {
                    connection_id: "integration".into(),
                    key: array_key.clone(),
                    mode: ArrayCreateMode::Contiguous,
                    start_index: Some("0".into()),
                    values: vec!["one".into(), "two".into()],
                    elements: Vec::new(),
                    ttl_ms: None,
                })
                .await
                .map_err(|error| error.code().to_owned())?;
            if created.key != array_key {
                return Err("Array create returned an unexpected key".to_owned());
            }
            let range = service
                .get_array_range(ArrayRangeInput {
                    connection_id: "integration".into(),
                    key: array_key.clone(),
                    start: "0".into(),
                    end: "1".into(),
                })
                .await
                .map_err(|error| error.code().to_owned())?;
            if range.cells.len() != 2 {
                return Err("Array range returned an unexpected number of cells".to_owned());
            }
        } else {
            eprintln!("skipped: Redis Array module is not installed");
        }

        if capabilities.vector_set_supported {
            let created = service
                .create_vector_set(CreateVectorSetInput {
                    connection_id: "integration".into(),
                    key: vector_key.clone(),
                    dimension: 3,
                    quantization: Some("f32".into()),
                    elements: vec![
                        VectorSetElementPayload {
                            name: "one".into(),
                            vector_values: Some(vec![1.0, 0.0, 0.0]),
                            vector_fp32_base64: None,
                            attributes: Some(serde_json::json!({"kind": "seed"})),
                        },
                        VectorSetElementPayload {
                            name: "two".into(),
                            vector_values: Some(vec![0.0, 1.0, 0.0]),
                            vector_fp32_base64: None,
                            attributes: None,
                        },
                    ],
                    ttl_ms: None,
                })
                .await
                .map_err(|error| error.code().to_owned())?;
            if created.key != vector_key {
                return Err("Vector Set create returned an unexpected key".to_owned());
            }
            let page = service
                .list_vector_set_elements(ListVectorSetElementsInput {
                    connection_id: "integration".into(),
                    key: vector_key.clone(),
                    start: None,
                    end: None,
                    limit: 2,
                })
                .await
                .map_err(|error| error.code().to_owned())?;
            if page.elements.len() != 2 {
                return Err("Vector Set listing returned an unexpected number of elements".to_owned());
            }
        } else {
            eprintln!("skipped: Redis Vector Set module is not installed");
        }

        if !capabilities.array_supported && !capabilities.vector_set_supported {
            eprintln!("skipped: Array and Vector Set commands are unavailable");
        }
        Ok::<(), String>(())
    }
    .await;

    let cleanup = async {
        let mut errors = Vec::new();
        if let Err(error) = service.delete_key("integration", &array_key).await {
            errors.push(format!("Array cleanup: {}", error.code()));
        }
        if let Err(error) = service.delete_key("integration", &vector_key).await {
            errors.push(format!("Vector Set cleanup: {}", error.code()));
        }
        if let Err(error) = service.close_connection("integration").await {
            errors.push(format!("connection cleanup: {}", error.code()));
        }
        if errors.is_empty() {
            Ok::<(), String>(())
        } else {
            Err(errors.join("; "))
        }
    }
    .await;

    match (flow, cleanup) {
        (Ok(()), Ok(())) => {}
        (Err(flow), Ok(())) => panic!("Redis Stack Array/Vector Set flow failed: {flow}"),
        (Ok(()), Err(cleanup)) => panic!("Redis Stack Array/Vector Set cleanup failed: {cleanup}"),
        (Err(flow), Err(cleanup)) => {
            panic!("Redis Stack Array/Vector Set flow failed: {flow}; cleanup also failed: {cleanup}")
        }
    }
}
```

- [ ] **Step 2: 运行验证入口**

运行：

```bash
cargo test --manifest-path src-tauri/Cargo.toml redis_stack_array_and_vector_set_flow_when_redis_stack_is_available -- --exact
npm run check:non-cloud
```

预期：没有 Redis 地址时 ignored 测试必须明确 skip；配置 Redis 地址时 smoke flow 必须真实调用 typed service 方法。文档矩阵在实现完成前应暴露未同步条目，不能把 skip 说成网络通过。

- [ ] **Step 3: 完成集成流程、文档和历史记录**

集成流程在 capability 支持时执行：

1. Array：创建连续和稀疏数据，读取 ARLEN/ARCOUNT/ARNEXT，执行范围/扫描、单元素设置、追加、ARGREP、AROP、按索引删除和按区间删除。
2. Vector Set：创建至少两个三维元素，读取 VCARD/VINFO/VRANGE/VEMB/VGETATTR，更新/删除属性，执行 element/VALUES/FP32 三种 VSIM，删除元素。
3. 每个操作通过 `map_err(|error| error.code().to_owned())` 转换错误；能力缺失只 skip 对应模块，普通 Redis 功能和另一个模块继续验证。
4. cleanup 使用已创建 key 的显式列表执行删除，随后关闭连接；cleanup 错误和 flow 错误分别报告。

README 和 `docs/non-cloud-scope.md` 写明：

- 已实现本地 Array：创建、稀疏范围/扫描、编辑、追加、删除、搜索、聚合；
- 已实现本地 Vector Set：创建、元素/属性 CRUD、FP32 向量和 VSIM；
- Redis Cloud、Azure、RDI、AI/Copilot、Telemetry、远程插件、SSH/Sentinel/Cluster 和 SQL 仍排除；
- `REDIX_TEST_REDIS_STACK_URL` 未配置时 ignored 流程只记录 skip。

更新 `task_plan.md`、`findings.md`、`progress.md` 的任务状态和验证证据，记录未配置真实 Redis 时的限制；提交前只暂存本批次新增的状态/证据行，不能把这些文件中已有的用户修改一起提交。

- [ ] **Step 4: 运行全量验证**

运行：

```bash
npm run test:frontend
npm run build
npm run check:non-cloud
npm run test:rust
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
git diff --check
```

若环境已配置 Redis Stack，再运行：

```bash
cargo test --manifest-path src-tauri/Cargo.toml redis_stack_array_and_vector_set_flow_when_redis_stack_is_available -- --ignored --nocapture
```

验收要求：前端和 Rust 测试退出码为 0；构建、非 Cloud 检查、rustfmt 和 diff check 通过；没有 Redis 地址时必须把 ignored 流程记录为 skip，不声称真实网络断言通过。

- [ ] **Step 5: 提交**

```bash
git add src-tauri/tests/redis_integration.rs README.md docs/non-cloud-scope.md
git commit -m "完成 Array 和 Vector Set 全量验证"
```

## 完成判定

只有在 Task 1-8 的 checkbox 全部完成，并且以下条件同时成立时，才能宣称本批次完成：

- Rust domain、parser、service、command 和 persistence/connection 回归测试通过；
- 前端 typed bridge、Browser state、Array/Vector Set detail 和原有 Browser 回归测试通过；
- `npm run build`、`npm run check:non-cloud`、`npm run test:rust` 和 rustfmt/diff check 通过；
- 支持模块的 Redis Stack 流程已真实执行，或明确记录为环境未配置导致的 skip；
- 未修改参考项目 RedisInsight，未引入 Redis Cloud 或本计划排除的功能；
- 当前工作区的业务代码变更已按任务提交，提交信息均为简体中文。
