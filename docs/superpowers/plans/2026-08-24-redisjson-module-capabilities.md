# RedisJSON 模块能力与深层编辑实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在现有 Standalone Browser 根文档编辑之上，增加连接级 Redis 模块能力探测和 RedisJSON 路径级读取、设置、删除、数组追加，并在 RedisJSON 不可用时稳定降级。

**Architecture:** Rust 新增独立的 JSON path domain/parser/service，复用现有 `RedisService` 的 active client、TLS、错误映射和 typed IPC；能力探测只缓存于当前连接 session，成功的 JSON 动作在同一连接上执行并返回受限 DTO。React 在 Browser 中异步探测模块能力，把路径编辑封装为独立 `JsonPathEditor`，复用 `KeyDetails` 的连接/键 request-token 保护。

**Tech Stack:** Rust 2021、Tauri 2、`redis` 1.5 async multiplexed connection、Serde/serde_json、Tokio `RwLock`、React 19、TypeScript、Vitest、Testing Library。

**Spec:** `docs/superpowers/specs/2026-08-24-redisinsight-non-cloud-parity-modules-topology-design.md`

## Global Constraints

- 只实现本计划的 RedisJSON/模块能力第一批；RedisSearch/Query、Vector Set、Array、SSH、Sentinel、Cluster 和 Workbench/CLI 高级批次不在本计划中。
- RedisJSON capability 只通过当前连接的 `MODULE LIST` 探测，探测失败不能阻塞连接打开；未知或未安装模块时 UI 保持普通 Browser 可用并显示稳定降级提示。
- JSON path 固定最大 512 字节；JSON payload 固定最大 5 MiB；数组追加固定最多 500 个元素；完整 Redis 响应固定最大 4 MiB；所有值和限制在 Rust domain 测试中固定。
- 连接/键/路径/请求 token 发生变化时，前端必须丢弃旧响应；组件卸载时不能更新状态。
- Redis 命令必须经 Rust service 和 typed IPC 执行；不向前端暴露 `redis::Value`、密码、证书、连接地址或底层 Redis 错误文本。
- 不使用 `KEYS`、SQL、shell、本地脚本或远程插件；不得在循环遍历中查询 SQL。
- 生产源码中不得加入 Redis Cloud、Azure Managed Redis、RDI、AI、Telemetry、远程插件或云账户入口。
- 每个任务遵守 RED → GREEN → REFACTOR；每个任务完成后使用简体中文提交信息，默认在当前 `main` 分支修改。
- 真实 Redis Stack 流程使用环境变量并保持 ignored；未配置环境变量时只记录未执行，不把无网络测试声称为通过。

## Final Review Addendum

- 最终审查修复波次将 `validate_json_path()` 从黑名单改为白名单：只允许根路径、对象成员链和单个非负整数数组下标，允许合理的 bracket key quoting/escaping；union、slice、filter、recursive descent、function/operator 和其他多目标表达式必须拒绝。
- `JsonPathValue` 在 `value` 之外新增 `found: bool`，用来区分“路径不存在”和“路径存在但值为 JSON null”；前端保持对真实 `null` 的显示能力。
- `MODULE LIST` 能力探测使用严格 `Result` parser：空数组是合法“无模块”，但 malformed top-level / malformed entry / missing name / RESP2 奇数字段都必须返回 `AppError::CommandFailed`，不能缓存成 no modules；`database_analysis` 现有宽松 parser 不变。
- 成功的 RedisJSON mutation 后，`PTTL` 失败只降级为未知 TTL，不得把写入回滚成失败；前端 detail refresh 失败也不得覆盖已提交成功。
- capability cache 在 `open_connection`、`close_connection`、`select_database` 时递增 session generation；probe 返回后只有 token 仍匹配当前连接会话时才允许写回缓存。

## 文件地图与责任

第一批只修改下列业务文件和测试/文档文件，后续 Search/Vector/拓扑计划不得把状态继续塞入这些文件：

| 文件 | 责任 |
| --- | --- |
| `src-tauri/src/domain/module_capabilities.rs` | 模块能力 DTO、RedisJSON 能力判定和连接输入校验 |
| `src-tauri/src/domain/json_path.rs` | JSON path/JSON payload/数组追加输入校验与结果 DTO |
| `src-tauri/src/redis/json_ops.rs` | RESP2/RESP3 模块列表解析、RedisJSON reply 归一化和命令 helper |
| `src-tauri/src/redis/connection_manager.rs` | capability session cache、RedisOperations trait 和 active client 生命周期接入 |
| `src-tauri/src/commands/json.rs` | Tauri JSON/module command adapter |
| `src/lib/types.ts`、`src/lib/tauri.ts` | 前端 DTO 与 typed IPC wrapper |
| `src/features/browser/JsonPathEditor.tsx` | JSON path 读取/编辑/删除/数组追加 UI，不承载连接列表或全局 Browser 扫描状态 |
| `src/features/browser/BrowserPage.tsx`、`KeyDetails.tsx`、`browserState.ts` | capability 探测、降级状态、request-token 传递和详情刷新 |
| `src/features/browser/*.test.tsx`、`src/lib/tauri.test.ts`、`src-tauri/tests/*.rs` | 单元、组件、IPC 和 ignored Redis Stack 流程 |
| `README.md`、`docs/non-cloud-scope.md` | 更新已支持 RedisJSON path、TLS 和仍排除的后续能力 |

未来批次的计划文件：`2026-08-24-search-query.md`、`2026-08-24-vector-array.md`、`2026-08-24-topology.md`；本计划不创建这些文件。

---

### Task 1: 建立模块能力与 JSON path domain 合同

**Files:**
- Create: `src-tauri/src/domain/module_capabilities.rs`
- Create: `src-tauri/src/domain/json_path.rs`
- Modify: `src-tauri/src/domain/mod.rs`
- Test: `src-tauri/tests/domain.rs`

**Interfaces:**
- Consumes: 现有 `ModuleSummary`、`AppError`、`serde_json::Value`。
- Produces: 后续 Redis service、Tauri commands 和 TypeScript bridge 使用的稳定 Rust DTO。

Rust DTO 必须保持 snake_case 序列化字段，并实现 `Clone + Debug + Serialize + Deserialize`；含 JSON value 的 DTO 实现 `PartialEq` 而不强求 `Eq`。接口固定为：

```rust
pub struct GetModuleCapabilitiesInput {
    pub connection_id: String,
}

pub struct ModuleCapabilities {
    pub modules: Vec<ModuleSummary>,
    pub json_supported: bool,
    pub json_version: Option<String>,
}

pub struct GetJsonPathInput {
    pub connection_id: String,
    pub key: String,
    pub path: String,
}

pub struct SetJsonPathInput {
    pub connection_id: String,
    pub key: String,
    pub path: String,
    pub value: serde_json::Value,
}

pub struct AppendJsonArrayInput {
    pub connection_id: String,
    pub key: String,
    pub path: String,
    pub values: Vec<serde_json::Value>,
}

pub struct DeleteJsonPathInput {
    pub connection_id: String,
    pub key: String,
    pub path: String,
}

pub struct JsonPathValue {
    pub key: String,
    pub path: String,
    pub value: Option<serde_json::Value>,
    pub ttl_ms: i64,
}

pub struct JsonMutationResult {
    pub key: String,
    pub path: String,
    pub affected: u64,
    pub new_length: Option<u64>,
    pub ttl_ms: i64,
}
```

- [ ] **Step 1: Write failing validation tests**

Add tests to `src-tauri/tests/domain.rs` with these exact behaviors:

```rust
#[test]
fn json_path_inputs_reject_empty_or_unsafe_paths() {
    let input = GetJsonPathInput {
        connection_id: "local".into(),
        key: "doc".into(),
        path: "$.items[*]".into(),
    };

    assert_eq!(input.validate().unwrap_err(), AppError::InvalidInput);
    assert_eq!(validate_json_path("", false), Err(AppError::InvalidInput));
    assert_eq!(validate_json_path("$.user.name", false), Ok(()));
}

#[test]
fn json_path_supports_legacy_and_modern_root_forms() {
    assert_eq!(normalize_json_path("$", true).unwrap(), ".");
    assert_eq!(normalize_json_path("$.user", true).unwrap(), ".user");
    assert_eq!(normalize_json_path("$.user", false).unwrap(), "$.user");
}

#[test]
fn json_payload_and_array_append_limits_are_enforced() {
    let oversized = serde_json::Value::String("x".repeat(5 * 1024 * 1024));
    let input = SetJsonPathInput {
        connection_id: "local".into(),
        key: "doc".into(),
        path: "$".into(),
        value: oversized,
    };

    assert_eq!(input.validate().unwrap_err(), AppError::InvalidInput);
    assert_eq!(validate_json_array_append(&[]), Err(AppError::InvalidInput));
}
```

- [ ] **Step 2: Run the focused tests and verify RED**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --test domain json_path -- --nocapture
```

Expected: FAIL because `GetJsonPathInput`, `validate_json_path`, `normalize_json_path` and the JSON path DTO validation do not exist yet. The failure must be a compile/test failure caused by the missing contract, not a malformed test.

- [ ] **Step 3: Implement the minimal domain contract**

Implement the DTOs and validators with these rules:

- `connection_id` and `key` must be non-empty after trimming.
- Path length is measured in UTF-8 bytes and must be `1..=512`.
- A path must start with `$` or `.`; reject control characters, `*`, `..`, `?`, and `;` so the first UI cannot request multi-match or expression-like paths.
- `normalize_json_path(path, legacy)` converts `$` to `.` and strips the first `$` from `$.field` only for legacy RedisJSON; modern paths remain unchanged.
- Serialize JSON values with `serde_json::to_vec`; reject payloads over 5 MiB.
- Array append requires `1..=500` values and applies the same aggregate 5 MiB payload limit.
- `ModuleCapabilities::from_modules` recognizes case-insensitive `ReJSON` and `RedisJSON`; keep the first non-empty version and set `json_supported` only for those module names.

Add the modules to `src-tauri/src/domain/mod.rs` and re-export their public DTOs. Do not change the existing `RedisValue::Json` root document contract.

- [ ] **Step 4: Run the focused tests and verify GREEN**

Run the same `cargo test` command from Step 2, then run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --test domain
```

Expected: the new validation tests and all existing domain tests pass.

- [ ] **Step 5: Commit the domain contract**

```bash
git add src-tauri/src/domain/module_capabilities.rs src-tauri/src/domain/json_path.rs src-tauri/src/domain/mod.rs src-tauri/tests/domain.rs
git commit -m "补充 RedisJSON 模块与路径领域协议"
```

### Task 2: 实现 capability cache 与 RedisJSON command helper

**Files:**
- Create: `src-tauri/src/redis/json_ops.rs`
- Modify: `src-tauri/src/redis/mod.rs`
- Modify: `src-tauri/src/redis/connection_manager.rs`
- Test: `src-tauri/src/redis/json_ops.rs` (unit tests)
- Test: `src-tauri/tests/redis_integration.rs` (ignored Redis Stack flow)

**Interfaces:**
- Consumes: Task 1 DTO/validators, existing `parse_module_list`, `map_command_error`/`map_json_command_error`, active TLS-aware `Client`。
- Produces: `RedisOperations` methods `get_module_capabilities`, `get_json_path`, `set_json_path`, `append_json_array`, `delete_json_path` and parser helpers used by commands。

Add the following trait methods to `RedisOperations`:

```rust
async fn get_module_capabilities(
    &self,
    connection_id: &str,
) -> Result<ModuleCapabilities, AppError>;
async fn get_json_path(&self, input: GetJsonPathInput) -> Result<JsonPathValue, AppError>;
async fn set_json_path(&self, input: SetJsonPathInput) -> Result<JsonMutationResult, AppError>;
async fn append_json_array(
    &self,
    input: AppendJsonArrayInput,
) -> Result<JsonMutationResult, AppError>;
async fn delete_json_path(
    &self,
    input: DeleteJsonPathInput,
) -> Result<JsonMutationResult, AppError>;
```

- [ ] **Step 1: Write failing parser and cache lifecycle tests**

In `src-tauri/src/redis/json_ops.rs`, add tests for:

```rust
#[test]
fn parses_resp2_and_resp3_module_lists_for_json_capability() {
    let resp2 = Value::Array(vec![Value::Array(vec![
        Value::BulkString(b"name".to_vec()),
        Value::BulkString(b"ReJSON".to_vec()),
        Value::BulkString(b"ver".to_vec()),
        Value::Int(20000),
    ])]);
    let resp3 = Value::Attribute {
        data: Box::new(Value::Array(vec![Value::Map(vec![
            (Value::SimpleString("name".into()), Value::SimpleString("RedisJSON".into())),
            (Value::SimpleString("ver".into()), Value::Int(20000)),
        ])])),
        attributes: vec![],
    };

    assert!(parse_module_capabilities(resp2).json_supported);
    assert!(parse_module_capabilities(resp3).json_supported);
}

#[test]
fn normalizes_json_get_root_reply_without_unwrapping_nested_arrays() {
    let object_reply = Some(r#"[{"name":"redix"}]"#.to_owned());
    let object = parse_json_get_reply(object_reply, "$", false).unwrap().unwrap();
    assert_eq!(object, serde_json::json!({"name": "redix"}));

    let array_reply = Some(r#"[[{"name":"redix"}]]"#.to_owned());
    let array = parse_json_get_reply(array_reply, "$", false).unwrap().unwrap();
    assert_eq!(array, serde_json::json!([{"name": "redix"}]));
}

#[test]
fn missing_json_reply_is_not_reported_as_a_server_error() {
    assert_eq!(parse_json_get_reply(None, "$.missing", false).unwrap(), None);
}
```

Add an ignored integration test named `redis_stack_json_path_flow_when_redis_stack_is_available` that is skipped unless `REDIX_TEST_REDIS_STACK_URL` is present. It must create one unique `redix:json-path:<pid>:<timestamp>` key, call capability probe, return early with a recorded skip when JSON is not installed, then exercise nested read/set, array append, delete, and final cleanup with `DEL` on the owned key only.

- [ ] **Step 2: Run parser tests and verify RED**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml json_ops -- --nocapture
```

Expected: FAIL because `parse_module_capabilities` and `parse_json_get_reply` are not defined. Do not use a live Redis connection for this RED check.

- [ ] **Step 3: Implement parser and Redis command helpers**

In `json_ops.rs`:

- Parse `MODULE LIST` using the existing `parse_module_list`; unwrap RESP3 `Attribute`, accept RESP2 array and RESP3 map entries, and normalize integer `ver` to a dotted major-version string only when needed for legacy detection.
- Treat a missing JSON version as legacy, matching the reference project's safe fallback. `version.starts_with("1.")` uses legacy path syntax; all other known versions use modern `$` paths.
- Parse `JSON.GET` `Option<String>` with `serde_json`; for a modern root `$`, unwrap only the RedisJSON response wrapper `[document]`, preserving a document whose actual value is an array. Return `None` for nil/missing path and `COMMAND_FAILED` for malformed JSON.
- Add helpers that issue `JSON.GET`, `JSON.SET`, `JSON.ARRAPPEND`, and `JSON.DEL` with arguments as separate Redis command arguments. Serialize each JSON value with compact `serde_json::to_string` and map unknown command/wrong-type errors through `map_json_command_error`.
- Read `PTTL` after every mutation and return `JsonMutationResult`; parse `JSON.ARRAPPEND` modern array replies and legacy integer replies into `new_length`.

In `connection_manager.rs`:

- Add `capabilities: Arc<RwLock<HashMap<String, ModuleCapabilities>>>` to `RedisService::new`.
- `get_module_capabilities` first returns the cached snapshot, otherwise opens the active connection, issues `MODULE LIST`, parses it, caches only successful results, and returns `COMMAND_FAILED` on probe failure. It must never be called from `open_connection`.
- Remove the connection id from the cache before/when `open_connection` replaces a client and from `close_connection`; Pub/Sub/Profiler cleanup remains unchanged.
- JSON methods validate their input, resolve module capabilities, return `UNSUPPORTED_DATA_TYPE` when JSON is absent, use a single connection for the JSON command plus PTTL/refresh reply, and preserve existing `get_key/set_key` root behavior。

Register `mod json_ops;` and keep helper functions `pub(crate)` unless the integration test needs a public re-export. Do not add a second Redis client or a second TLS builder.

- [ ] **Step 4: Run parser and service tests and verify GREEN**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml json_ops -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test domain
cargo test --manifest-path src-tauri/Cargo.toml --test redis_integration redis_stack_json_path_flow_when_redis_stack_is_available -- --ignored --nocapture
```

Expected: parser/service unit tests pass; the integration command is either explicitly skipped because `REDIX_TEST_REDIS_STACK_URL` is unset or runs the unique-key Redis Stack flow and cleans only its own key.

- [ ] **Step 5: Commit the Redis service**

```bash
git add src-tauri/src/redis/json_ops.rs src-tauri/src/redis/mod.rs src-tauri/src/redis/connection_manager.rs src-tauri/tests/redis_integration.rs
git commit -m "实现 RedisJSON 能力探测与路径服务"
```

### Task 3: 接入 typed IPC 与 Tauri command

**Files:**
- Create: `src-tauri/src/commands/json.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/tests/commands.rs`
- Modify: `src/lib/types.ts`
- Modify: `src/lib/tauri.ts`
- Modify: `src/lib/tauri.test.ts`

**Step 1: Write the failing test**

在 `src/lib/tauri.test.ts` 增加 IPC 输入输出测试，先锁定所有 wrapper 不直接暴露 Tauri 底层调用：

    it("为 RedisJSON 路径操作传递 snake_case command 和 input", async () => {
      const input = { connection_id: "local", key: "doc", path: "$.user" };
      const result = { key: "doc", path: "$.user", value: { name: "redix" }, ttl_ms: -1 };
      invokeMock.mockResolvedValue(result);

      await expect(getJsonPath(input)).resolves.toEqual(result);
      expect(invokeMock).toHaveBeenLastCalledWith("get_json_path", { input });
    });

同时增加模块能力探测的 direct `connection_id` 断言，以及 set/append/delete 三个 mutation wrapper 的 `{ input }` 断言；在 Rust command adapter 测试中增加五个新 command 的函数引用。

**Step 2: Run test to verify it fails**

运行 `npm run test:frontend -- src/lib/tauri.test.ts` 和 `cargo test --manifest-path src-tauri/Cargo.toml --test commands exposes_all_tauri_command_adapters`，确认新 wrapper、类型和 command 尚不存在时失败。

**Step 3: Write minimal implementation**

在 `src/lib/types.ts` 增加与 Rust DTO 一一对应的类型：

    export interface ModuleCapabilities {
      modules: ModuleSummary[];
      json_supported: boolean;
      json_version: string | null;
    }

    export interface JsonPathInput {
      connection_id: string;
      key: string;
      path: string;
    }

    export interface SetJsonPathInput extends JsonPathInput {
      value: JsonValue;
    }

    export interface AppendJsonArrayInput extends JsonPathInput {
      values: JsonValue[];
    }

    export interface JsonPathValue {
      key: string;
      path: string;
      value: JsonValue | null;
      ttl_ms: number;
    }

    export interface JsonMutationResult {
      key: string;
      path: string;
      affected: number;
      new_length: number | null;
      ttl_ms: number;
    }

在 `src/lib/tauri.ts` 增加：

- `getModuleCapabilities(connectionId: string)`：调用 `get_module_capabilities` 并传递 `{ connection_id: connectionId }`。
- `getJsonPath(input: JsonPathInput)`：调用 `get_json_path` 并传递 `{ input }`。
- `setJsonPath(input: SetJsonPathInput)`、`appendJsonArray(input: AppendJsonArrayInput)`、`deleteJsonPath(input: JsonPathInput)`：分别调用对应 snake_case command 并传递 `{ input }`。

在 `src-tauri/src/commands/json.rs` 添加五个窄适配器：模块能力探测直接接收 `connection_id: String`；四个 JSON 路径 command 接收 `input`，只负责调用 `RedisService` trait，不在 command 层解析 Redis RESP 或拼接路径。随后在 `commands/mod.rs`、`lib.rs` handler 和 `tests/commands.rs` 注册它们。

**Step 4: Run tests to verify it passes**

运行 `npm run test:frontend -- src/lib/tauri.test.ts`、`npm run build`、`cargo test --manifest-path src-tauri/Cargo.toml --test commands`，确认 typed IPC 和 Tauri 注册完整。

**Step 5: Commit**

提交：`git commit -m "接入 RedisJSON 路径 typed IPC"`。

### Task 4: 增加 Browser RedisJSON 路径编辑器

**Files:**
- Create: `src/features/browser/JsonPathEditor.tsx`
- Create: `src/features/browser/JsonPathEditor.test.tsx`
- Modify: `src/features/browser/BrowserPage.tsx`
- Modify: `src/features/browser/KeyDetails.tsx`
- Modify: `src/features/browser/browserState.ts`
- Modify: `src/features/browser/browser.test.tsx`
- Modify: `src/app.smoke.test.tsx`
- Modify: `src/features/workbench/workbench.test.tsx`
- Modify: `src/styles.css`

**Step 1: Write the failing test**

在 `JsonPathEditor.test.tsx` 先覆盖：

- 读取路径调用 `onRead` 并展示返回的值、TTL 和 path；
- 非法 JSON / set 空值、append 非数组、delete 空路径均显示校验错误且不调用 mutation；
- set、数组追加、delete 分别传递正确的 typed mutation；
- `busy` 时禁用输入和按钮，异步错误显示在编辑器内。

在 `browser.test.tsx` 增加模块探测成功、模块探测失败但仍能扫描，以及 JSON key 渲染路径编辑器和 mutation 后刷新详情的失败测试。

测试所需的组件边界固定为：

    export type JsonPathMutation =
      | { kind: "set"; path: string; value: JsonValue }
      | { kind: "append"; path: string; values: JsonValue[] }
      | { kind: "delete"; path: string };

    interface JsonPathEditorProps {
      value: JsonValue;
      busy: boolean;
      error: string | null;
      onRead(path: string): Promise<JsonPathValue>;
      onMutate(mutation: JsonPathMutation): Promise<JsonMutationResult>;
    }

**Step 2: Run test to verify it fails**

运行 `npm run test:frontend -- src/features/browser/JsonPathEditor.test.tsx src/features/browser/browser.test.tsx`，确认组件、探测状态和回调尚未实现时失败。

**Step 3: Write minimal implementation**

创建 `JsonPathEditor.tsx`，提供 path 输入（默认 `$`）、JSON value 文本框，以及 `读取路径`、`保存路径`、`数组追加`、`删除路径` 四个操作。组件只负责表单校验、JSON parse 和回调调用，不直接访问 Tauri；错误使用现有 Browser 文案模式展示。

在 `BrowserPage.tsx` 增加页面级模块探测状态：

    type ModuleProbeState =
      | { status: "loading"; capabilities: null }
      | { status: "ready"; capabilities: ModuleCapabilities }
      | { status: "failed"; capabilities: null };

页面加载时调用 `getModuleCapabilities(connectionId)`；失败只记录探测失败并继续执行扫描，不阻塞 Browser。将探测状态传给 `KeyDetails`。

在 `KeyDetails.tsx` 中仅当 key 为 JSON、探测状态为 ready 且 `json_supported` 为 true 时渲染 `JsonPathEditor`；探测失败或模块不可用时保留现有 root editor，并显示非阻断提示。所有 read/mutate callback 都复用现有 `OperationContext` token、connection id 和 key 校验；mutation 成功后调用现有 `refreshDetail` 与 `onDetailChange`，避免旧请求覆盖新详情。editor key 同时包含 detail 和探测状态，防止切换 key 时复用旧 path 草稿。

在 `browserState.ts` 补充 JsonValue、模块探测失败和 JSON 路径错误的用户文案映射；在 `styles.css` 沿用现有表单布局、危险操作按钮和错误状态样式，不引入新 UI 依赖。

**Step 4: Run tests to verify it passes**

运行 `npm run test:frontend -- src/features/browser/JsonPathEditor.test.tsx src/features/browser/browser.test.tsx`、`npm run build`，确认探测失败不阻断 Browser，路径读写和 root editor 均可用。

**Step 5: Commit**

提交：`git commit -m "增加 Browser RedisJSON 路径编辑器"`。

### Task 5: 更新范围文档并执行第一批完整验证

**Files:**
- Modify: `README.md`
- Modify: `docs/non-cloud-scope.md`
- Modify: `task_plan.md`
- Modify: `findings.md`
- Modify: `progress.md`

**Step 1: Update user-facing documentation**

更新 README 的能力矩阵：移除“TLS 不支持”这一过时描述，补充 RedisJSON 路径读写、模块探测和能力降级行为；同时明确当前范围仍排除 Redis Cloud、Azure Managed Redis、RDI、AI/Copilot、Telemetry、远程插件和云端登录/账户能力。更新 `docs/non-cloud-scope.md`，记录第一批已实现项、未实现项和 Redis Stack 集成测试的可选环境变量。

**Step 2: Run the complete verification suite**

    npm run test:frontend
    npm run build
    npm run check:non-cloud
    npm run test:rust
    cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
    git diff --check

若环境配置了 Redis Stack，仅额外执行：

    if [ -n "${REDIX_TEST_REDIS_STACK_URL:-}" ]; then
      cargo test --manifest-path src-tauri/Cargo.toml --test redis_integration redis_stack_json_path_flow_when_redis_stack_is_available -- --ignored --nocapture
    else
      echo "未配置 REDIX_TEST_REDIS_STACK_URL，Redis Stack 集成流程保持未执行"
    fi

**Step 3: Review the final changed-file set**

    git diff --stat
    git diff -- src-tauri/src/domain src-tauri/src/redis src-tauri/src/commands src/lib src/features/browser README.md docs/non-cloud-scope.md task_plan.md findings.md progress.md
    git status --short

确认没有 Redis Cloud 相关文件、秘密、临时构建产物或未登记的行为变更。

**Step 4: Commit**

提交：`git commit -m "完成 RedisJSON 第一批验证与范围文档"`。

## Plan Self-Review

- 覆盖范围：计划从领域 DTO、Redis 服务、typed IPC、Browser UI 到文档和验证，包含每个新增文件和注册点。
- TDD 顺序：每个实现任务都先给出失败测试和运行命令，再描述最小实现与绿色验证。
- 依赖一致性：Rust `snake_case` 字段、TypeScript 类型、Tauri 参数包装和现有 token 并发模型已逐项对齐。
- 安全边界：路径、payload、数组长度、模块探测失败和 stale response 均有明确约束；没有在循环中查询 SQL，也没有引入 SQL 能力。
- 范围边界：本计划只实现第一批 RedisJSON/模块能力基础；Search/Query、Vector/Array、Workbench/CLI、SSH/Sentinel/Cluster 各自保留独立后续计划。
- 可执行性：每项任务均已确定；每项任务都有明确文件、测试、命令和中文提交信息。
