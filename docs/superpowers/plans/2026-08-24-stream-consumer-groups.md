# Stream Consumer Group Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在本地 Redis Standalone Browser 的 Stream 键详情中补齐 Consumer Group、消费者、PENDING 观察和 XACK/消费者删除操作。

**Architecture:** 新增独立 `domain/stream.rs` DTO 与 `redis/stream_groups.rs` RESP parser；`RedisService` 通过当前已打开的 multiplexed connection 执行 XINFO/XGROUP/XPENDING/XACK，Tauri 使用 request-response typed IPC。React 新增聚焦 Stream 键的 `StreamConsumerGroups` 子组件，使用 operation token 过滤键切换后的旧请求，不创建后台消费任务。

**Tech Stack:** Rust stable、redis 1.5、Tauri 2、React 19、TypeScript、Vitest、Testing Library。

**Spec:** `docs/superpowers/specs/2026-08-24-stream-consumer-groups-design.md`

## Global Constraints

- 只支持当前已打开的本地 Redis Standalone 连接，不实现 Redis Cloud、Azure、Cluster/Sentinel fan-out、TLS/SSH 或 SQL。
- 不实现 `XREADGROUP`、阻塞消费、`XCLAIM`/`XAUTOCLAIM`、实时事件和后台 Consumer 任务。
- 所有输入在 Rust 校验；单个 Group/consumer/name 最长 256 字符，Pending count 最大 500，ack entries 最大 500。
- Redis 原始错误、原始响应、连接 URI 和密码不返回到前端；统一使用现有固定 `AppError`/`browserErrorMessage` 边界。
- 不在循环中执行 SQL；本批不引入 SQL。
- 默认在当前 `main` 分支修改，所有 commit message 使用简体中文。

---

### Task 1: Stream Consumer Group 领域 DTO 与 RESP parser

**Files:**
- Create: `src-tauri/src/domain/stream.rs`
- Create: `src-tauri/src/redis/stream_groups.rs`
- Modify: `src-tauri/src/domain/mod.rs`
- Modify: `src-tauri/src/redis/mod.rs`
- Test: `src-tauri/tests/domain.rs`
- Test: `src-tauri/src/redis/stream_groups.rs` (`#[cfg(test)]`)

**Interfaces:**
- Produces `GetStreamConsumerGroupsInput`, `StreamConsumerGroup`, `CreateStreamConsumerGroupInput`, `DeleteStreamConsumerGroupInput`, `GetStreamConsumersInput`, `StreamConsumer`, `GetStreamPendingEntriesInput`, `StreamPendingEntry`, `AcknowledgeStreamPendingEntriesInput`, `DeleteStreamConsumerInput`.
- Produces `parse_stream_consumer_groups`, `parse_stream_consumers`, `parse_stream_pending_entries` with `Result<..., AppError>` return types.
- Later tasks import these types from `crate::domain` and the parser functions from `crate::redis`.

- [ ] **Step 1: Write failing validation and parser tests**

Add tests with these exact cases:

```rust
#[test]
fn validates_stream_consumer_group_inputs() {
    assert_eq!(GetStreamConsumerGroupsInput {
        connection_id: "".into(), key: "events".into()
    }.validate().unwrap_err(), AppError::InvalidConnection);
    assert_eq!(GetStreamPendingEntriesInput {
        connection_id: "local".into(), key: "events".into(),
        group: "workers".into(), count: 501, consumer: None,
    }.validate().unwrap_err(), AppError::InvalidConnection);
    assert_eq!(AcknowledgeStreamPendingEntriesInput {
        connection_id: "local".into(), key: "events".into(),
        group: "workers".into(), entries: vec![],
    }.validate().unwrap_err(), AppError::InvalidConnection);
}
```

Parser tests must cover RESP2 flat key/value arrays, RESP3 maps wrapped in Attribute, nested Pending arrays, a missing required field, and a malformed delivery count. Assert only the fixed `COMMAND_FAILED` code/message for failures.

- [ ] **Step 2: Run focused tests and verify RED**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml stream_consumer_group
cargo test --manifest-path src-tauri/Cargo.toml stream_groups
```

Expected: FAIL because the DTOs, validation methods and parser functions do not exist yet.

- [ ] **Step 3: Implement DTOs and validation**

Create the DTOs with `serde::Serialize`, `serde::Deserialize`, `Debug`, `Clone`, and `PartialEq` where applicable. Use `AppError::InvalidConnection` for invalid Browser inputs. Every validator must reject blank connection/key/group/name/consumer values, enforce the 256-character single-name limit, enforce `1..=500` for Pending count, and require `1..=500` nonblank IDs for ack.

```rust
pub struct StreamPendingEntry {
    pub id: String,
    pub consumer: String,
    pub idle_ms: u64,
    pub deliveries: u64,
}
```

Export the new module from `domain/mod.rs`.

- [ ] **Step 4: Implement safe RESP parsers**

In `redis/stream_groups.rs`, unwrap `Value::Attribute { data, .. }` recursively, accept `Array`, `Set`, and `Map` containers, and read XINFO fields by field name. Use private helpers `value_to_string`, `value_to_u64`, `pair_fields`, and `unwrap_container`; any conversion error returns `AppError::CommandFailed` without including the input value.

```rust
pub fn parse_stream_consumer_groups(
    value: redis::Value,
) -> Result<Vec<StreamConsumerGroup>, AppError>;
pub fn parse_stream_consumers(
    value: redis::Value,
) -> Result<Vec<StreamConsumer>, AppError>;
pub fn parse_stream_pending_entries(
    value: redis::Value,
) -> Result<Vec<StreamPendingEntry>, AppError>;
```

`XPENDING` detail rows must contain exactly four convertible fields: ID, consumer, idle milliseconds and delivery count. `XINFO` replies must contain their required `name` field; groups also require `consumers`, `pending`, and `last-delivered-id`.

- [ ] **Step 5: Run focused tests and verify GREEN**

Run:

```bash
cargo fmt --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml stream_consumer_group
cargo test --manifest-path src-tauri/Cargo.toml stream_groups
```

Expected: all new validation and parser tests pass.

- [ ] **Step 6: Commit the domain/parser task**

```bash
git add src-tauri/src/domain/stream.rs src-tauri/src/domain/mod.rs \
  src-tauri/src/redis/stream_groups.rs src-tauri/src/redis/mod.rs \
  src-tauri/tests/domain.rs
git commit -m "增加 Stream Consumer Group 领域模型和解析"
```

### Task 2: Redis service commands and connection-bound execution

**Files:**
- Modify: `src-tauri/src/redis/connection_manager.rs`
- Modify: `src-tauri/tests/redis_integration.rs`
- Test: `src-tauri/src/redis/connection_manager.rs` (`#[cfg(test)]`)

**Interfaces:**
- Adds these `RedisOperations` methods:

```rust
async fn get_stream_consumer_groups(
    &self, input: GetStreamConsumerGroupsInput,
) -> Result<Vec<StreamConsumerGroup>, AppError>;
async fn create_stream_consumer_group(
    &self, input: CreateStreamConsumerGroupInput,
) -> Result<(), AppError>;
async fn delete_stream_consumer_group(
    &self, input: DeleteStreamConsumerGroupInput,
) -> Result<u64, AppError>;
async fn get_stream_consumers(
    &self, input: GetStreamConsumersInput,
) -> Result<Vec<StreamConsumer>, AppError>;
async fn get_stream_pending_entries(
    &self, input: GetStreamPendingEntriesInput,
) -> Result<Vec<StreamPendingEntry>, AppError>;
async fn acknowledge_stream_pending_entries(
    &self, input: AcknowledgeStreamPendingEntriesInput,
) -> Result<u64, AppError>;
async fn delete_stream_consumer(
    &self, input: DeleteStreamConsumerInput,
) -> Result<u64, AppError>;
```

- Consumes Task 1 DTOs/parser and existing `client`, `map_command_error`, and `tokenize` boundaries.
- Produces typed service behavior for the Tauri command layer; all methods require an active connection and validate before Redis access.

- [ ] **Step 1: Write failing service tests**

Add a unit test that constructs `RedisService` with empty repositories and verifies `get_stream_consumer_groups` returns `AppError::ConnectionFailed` for a valid input without an active connection. Add tests for invalid count/empty group reaching the validation boundary. Add an ignored integration test that creates an existing stream, creates a group, reads groups/consumers/PENDING, acknowledges an ID, deletes a consumer and destroys the group.

- [ ] **Step 2: Run service tests to verify RED**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml connection_manager::tests
cargo test --manifest-path src-tauri/Cargo.toml --test redis_integration runs_stream_consumer_group_flow
```

Expected: unit compilation fails because the trait methods are absent; the integration test remains explicitly ignored after it compiles.

- [ ] **Step 3: Implement the Redis service methods**

For each method, call `input.validate()` and then `self.connection(&input.connection_id).await?`. Use these command shapes:

```rust
redis::cmd("XINFO").arg("GROUPS").arg(&input.key)
redis::cmd("XGROUP").arg("CREATE").arg(&input.key)
    .arg(&input.name).arg(&input.last_delivered_id)
redis::cmd("XGROUP").arg("DESTROY").arg(&input.key).arg(&input.name)
redis::cmd("XINFO").arg("CONSUMERS").arg(&input.key).arg(&input.group)
redis::cmd("XPENDING").arg(&input.key).arg(&input.group)
    .arg("-").arg("+").arg(input.count)
redis::cmd("XACK").arg(&input.key).arg(&input.group).arg(&input.entries)
redis::cmd("XGROUP").arg("DELCONSUMER").arg(&input.key)
    .arg(&input.group).arg(&input.consumer)
```

Query results must be typed as `redis::Value`, parsed through Task 1 functions, and integer replies converted with `u64::try_from`. Map Redis errors through `map_command_error`; never return the Redis error string.

- [ ] **Step 4: Run service tests and compile the integration test**

Run:

```bash
cargo fmt --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml connection_manager::tests
cargo test --manifest-path src-tauri/Cargo.toml --test redis_integration runs_stream_consumer_group_flow
```

Expected: unit tests pass; the integration test compiles and reports ignored when no explicit `--ignored` flag is used.

- [ ] **Step 5: Commit the service task**

```bash
git add src-tauri/src/redis/connection_manager.rs src-tauri/tests/redis_integration.rs
git commit -m "实现 Stream Consumer Group Redis 服务"
```

### Task 3: Tauri command adapters and TypeScript bridge

**Files:**
- Modify: `src-tauri/src/commands/browser.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/tests/commands.rs`
- Modify: `src/lib/types.ts`
- Modify: `src/lib/tauri.ts`
- Modify: `src/lib/tauri.test.ts`

**Interfaces:**
- Produces seven Tauri commands with the exact names from the spec.
- Produces TypeScript types matching Task 1 snake_case fields and wrappers with signatures:

```ts
getStreamConsumerGroups(input: GetStreamConsumerGroupsInput): Promise<StreamConsumerGroup[]>;
createStreamConsumerGroup(input: CreateStreamConsumerGroupInput): Promise<void>;
deleteStreamConsumerGroup(input: DeleteStreamConsumerGroupInput): Promise<number>;
getStreamConsumers(input: GetStreamConsumersInput): Promise<StreamConsumer[]>;
getStreamPendingEntries(input: GetStreamPendingEntriesInput): Promise<StreamPendingEntry[]>;
acknowledgeStreamPendingEntries(input: AcknowledgeStreamPendingEntriesInput): Promise<number>;
deleteStreamConsumer(input: DeleteStreamConsumerInput): Promise<number>;
```

- [ ] **Step 1: Write failing command and bridge contract tests**

Add adapter exposure assertions for all seven Rust functions. Extend the Tauri bridge contract test with one input object for every wrapper and assert `invoke` is called with the exact snake_case command and `{ input }` shape; include representative group, consumer and pending response values.

- [ ] **Step 2: Run focused tests to verify RED**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --test commands exposes_all_tauri_command_adapters
npm test -- src/lib/tauri.test.ts
```

Expected: FAIL because the command adapters, types and wrappers are missing.

- [ ] **Step 3: Implement commands, registration, types and wrappers**

Each Rust adapter must be a thin `#[tauri::command]` function receiving `State<'_, AppState>` and the typed input, then calling the matching `RedisService` method. Register all seven in production `generate_handler!` and expose function pointers in command tests. Add wrappers through the existing private `call<T>` normalization path.

- [ ] **Step 4: Run focused tests and verify GREEN**

Run:

```bash
cargo fmt --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml --test commands
npm test -- src/lib/tauri.test.ts
npm run build
```

Expected: all command/bridge assertions pass and TypeScript production build succeeds.

- [ ] **Step 5: Commit the IPC task**

```bash
git add src-tauri/src/commands/browser.rs src-tauri/src/lib.rs src-tauri/tests/commands.rs \
  src/lib/types.ts src/lib/tauri.ts src/lib/tauri.test.ts
git commit -m "接入 Stream Consumer Group Tauri 桥接"
```

### Task 4: Browser Stream Consumer Groups workspace

**Files:**
- Create: `src/features/browser/StreamConsumerGroups.tsx`
- Create: `src/features/browser/StreamConsumerGroups.test.tsx`
- Modify: `src/features/browser/KeyDetails.tsx`
- Modify: `src/features/browser/browser.test.tsx`
- Modify: `src/styles.css`

**Interfaces:**
- Consumes Task 3 wrappers and `connectionId`/`streamKey` props.
- `StreamConsumerGroups` owns group, consumer and pending UI state; it receives no raw Redis client and emits no background event.
- KeyDetails renders the component only when `detail.key_type === "stream"` or `detail.value` is the `Stream` variant, passing the current connection/key identity.

- [ ] **Step 1: Write failing component tests**

Mock all seven bridge wrappers. Add tests that render the component and verify:

1. Group list loads and shows pending/consumer counts.
2. Create validates blank name, then calls `{ connection_id, key, name, last_delivered_id: "$" }` and refreshes.
3. Selecting a group loads consumers and Pending entries.
4. Selecting Pending rows and clicking acknowledge calls one `acknowledgeStreamPendingEntries` with all selected IDs, then refreshes.
5. Delete Group, delete consumer and clear-view paths require confirmation and call the correct wrapper.
6. A delayed response for the previous key is ignored after rerender with a new `streamKey`.

- [ ] **Step 2: Run component tests to verify RED**

Run:

```bash
npm test -- src/features/browser/StreamConsumerGroups.test.tsx
```

Expected: FAIL because the component and its UI do not exist.

- [ ] **Step 3: Implement the component**

Use these state boundaries:

```ts
const [groups, setGroups] = useState<StreamConsumerGroup[]>([]);
const [selectedGroup, setSelectedGroup] = useState<string | null>(null);
const [consumers, setConsumers] = useState<StreamConsumer[]>([]);
const [pending, setPending] = useState<StreamPendingEntry[]>([]);
const [selectedPending, setSelectedPending] = useState<string[]>([]);
const operationRef = useRef(0);
```

Increment `operationRef` on key/group changes and on unmount. Every async response checks both the operation token and current `connectionId/streamKey`. Keep pending rows capped at the requested 100 entries, use stable IDs for checkbox keys, and use `window.confirm` before destructive group/consumer deletion. Use `browserErrorMessage` for every rejected bridge call.

The layout contains a create form, group summary table, selected-group details, consumer table with delete actions, Pending table with row checkboxes and “确认选中”, and refresh buttons. Empty states must be explicit and buttons disabled while their operation is active.

- [ ] **Step 4: Integrate into KeyDetails and style it**

Render after `KeyEditor`:

```tsx
{detail.key_type === "stream" ? (
  <StreamConsumerGroups
    key={`${connectionId}:${detail.key}`}
    connectionId={connectionId}
    streamKey={detail.key}
  />
) : null}
```

Add compact styles for `.stream-consumer-groups`, group/consumer/pending tables, checkbox rows, responsive stacking and visible focus states. Keep existing detail panel spacing and light/dark tokens; do not add a new navigation workspace.

- [ ] **Step 5: Run Browser/component tests and verify GREEN**

Run:

```bash
npm test -- src/features/browser/StreamConsumerGroups.test.tsx src/features/browser/browser.test.tsx
npm run build
```

Expected: new component tests and all existing Browser tests pass; production build succeeds.

- [ ] **Step 6: Commit the Browser task**

```bash
git add src/features/browser/StreamConsumerGroups.tsx \
  src/features/browser/StreamConsumerGroups.test.tsx \
  src/features/browser/KeyDetails.tsx src/features/browser/browser.test.tsx src/styles.css
git commit -m "增加 Stream Consumer Group Browser 工作区"
```

### Task 5: Scope documentation and final regression

**Files:**
- Modify: `README.md`
- Modify: `docs/non-cloud-scope.md`
- Modify: `findings.md`
- Modify: `progress.md`
- Modify: `task_plan.md`

**Interfaces:**
- Documents the delivered local Stream Consumer Group capabilities and explicit exclusions.
- Does not change production behavior or add Cloud terms to scanned source entry points.

- [ ] **Step 1: Update scope and delivery records**

Add Stream Consumer Group to README and the allowed non-Cloud list. Keep `XREADGROUP`, `XCLAIM`, `XAUTOCLAIM`, live consumption, Cluster/Sentinel fan-out, TLS/SSH, modules, Cloud and SQL in exclusions. Record the design/plan paths, commits, ignored integration-test boundary and final test counts in the planning files.

- [ ] **Step 2: Run the full verification matrix**

Run:

```bash
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo test --manifest-path src-tauri/Cargo.toml
npm test
npm run build
npm run check:non-cloud
git diff --check
```

Expected: all ordinary Rust/frontend tests, build, non-Cloud scan, format and diff checks pass. The real Redis Consumer Group flow remains ignored unless explicitly authorized with a test Redis URL.

- [ ] **Step 3: Commit documentation and final verification**

```bash
git add README.md docs/non-cloud-scope.md findings.md progress.md task_plan.md
git commit -m "更新 Stream Consumer Group 非 Cloud 范围记录"
```
