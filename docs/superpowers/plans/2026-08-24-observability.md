# 本地 Redis 运维观察能力实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在当前 Redix 中补齐 RedisInsight 非 Cloud 的 Slow Log 与 Pub/Sub 本地 Redis 能力，并保证 Pub/Sub 事件任务可取消、错误安全、页面可用。

**Architecture:** Slow Log 通过现有 typed Tauri request-response 调用 Redis `SLOWLOG`/`CONFIG`；Pub/Sub 在 Rust `RedisService` 内为每个连接保存一个可取消的异步订阅任务，使用独立 Pub/Sub socket 读取消息并通过 Tauri `redix://pubsub/message`、`redix://pubsub/status` 事件发送固定 DTO。React 新增两个工作区，复用现有应用壳、CSS token、错误映射和连接上下文。

**Tech Stack:** Rust 2021、Tauri 2、redis 1.5 async PubSub、tokio、futures-util、serde、React 19、TypeScript、Vite、Vitest、Testing Library。

**Spec:** `docs/superpowers/specs/2026-08-24-observability-design.md`

## Global Constraints

- 默认在当前 `main` 分支直接修改，不创建新分支或 worktree。
- 只支持已打开的本地 Redis Standalone TCP；不实现 Redis Cloud、Azure、OAuth、云 API、AI、Telemetry、远程插件、Profiler、MONITOR、Cluster、Sentinel、TLS、SSH 或证书管理。
- Rust 不向前端暴露 Redis URI、密码、文件路径或底层错误文本；IPC 错误只保留固定 `code/message`。
- 继续不引入 SQL，也不得在循环遍历中查询 SQL；Redis 的多主题订阅循环只操作当前 Redis socket。
- 所有新增生产函数先有明确的失败测试并观察 RED，再写最小实现；每个任务完成后运行该任务的聚焦测试。
- UI 复用当前 RedisInsight 风格和已有 CSS token；保持深浅主题、键盘焦点、`aria-live` 状态和 375/768/1024/1440 宽度适配，不引入新图标库。
- Commit message 必须使用简体中文。

## 文件地图

### Rust domain/service/commands

- Create: `src-tauri/src/domain/observability.rs` — Slow Log、Pub/Sub DTO、输入校验和安全的 RESP 解析测试。
- Modify: `src-tauri/src/domain/mod.rs` — re-export observability DTO。
- Modify: `src-tauri/src/error.rs` — 增加 `INVALID_INPUT`、`OPERATION_CANCELLED` 固定错误。
- Create: `src-tauri/src/redis/observability.rs` — Slow Log reply/config 解析、Pub/Sub 任务生命周期 manager 和事件 payload 构造。
- Modify: `src-tauri/src/redis/mod.rs` — 注册 observability helper/module。
- Modify: `src-tauri/src/redis/connection_manager.rs` — 保存 manager、实现 Slow Log/发布/订阅方法、连接替换时清理任务。
- Modify: `src-tauri/Cargo.toml`, `src-tauri/Cargo.lock` — 引入已锁定的 `futures-util` 直接依赖。
- Create: `src-tauri/src/commands/observability.rs` — 7 个 Tauri command adapter。
- Modify: `src-tauri/src/commands/mod.rs`, `src-tauri/src/lib.rs` — 导出并注册命令。
- Modify: `src-tauri/tests/domain.rs`, `src-tauri/tests/commands.rs`, `src-tauri/tests/redis_integration.rs` — DTO、adapter 和显式 ignored 真实 Redis 流程。

### Frontend

- Modify: `src/lib/types.ts` — Slow Log、Pub/Sub DTO、workspace 类型和事件 payload 类型。
- Modify: `src/lib/tauri.ts` — 4 个 Slow Log wrapper、3 个 Pub/Sub invoke wrapper、两个 typed listener helper。
- Modify: `src/lib/tauri.test.ts` — IPC 命令名、参数形状、事件 listener 测试。
- Create: `src/features/observability/slowLogState.ts` — Slow Log 状态、校验、固定错误提示。
- Create: `src/features/observability/slowLogState.test.ts` — state helper 的 RED/GREEN 测试。
- Create: `src/features/observability/SlowLogPage.tsx`、`SlowLogPage.test.tsx` — Slow Log 页面和交互测试。
- Create: `src/features/observability/pubSubState.ts`、`pubSubState.test.ts` — 主题解析、session 过滤、5000 条缓存。
- Create: `src/features/observability/PubSubPage.tsx`、`PubSubPage.test.tsx` — Pub/Sub 页面和生命周期测试。
- Modify: `src/App.tsx`, `src/styles.css`, `src/app.smoke.test.tsx` — 导航、工作区、响应式样式和入口回归。
- Modify: `README.md`, `docs/non-cloud-scope.md` — 当前能力与明确边界。

---

### Task 1: DTO、校验与 Redis reply 解析

**Files:**
- Create: `src-tauri/src/domain/observability.rs`
- Modify: `src-tauri/src/domain/mod.rs`
- Modify: `src-tauri/src/error.rs`
- Create: `src-tauri/src/redis/observability.rs`
- Modify: `src-tauri/src/redis/mod.rs`
- Test: `src-tauri/tests/domain.rs`, `src-tauri/src/redis/observability.rs`

**Interfaces:**
- Produces `SlowLogEntry`, `SlowLogConfig`, `GetSlowLogsInput`, `UpdateSlowLogConfigInput`, `PubSubTopic`, `StartPubSubInput`, `PubSubSession`, `StopPubSubInput`, `PublishPubSubInput`, `PubSubMessageEvent`, `PubSubStatusEvent`。
- Produces `parse_slow_log_reply(value: redis::Value) -> Result<Vec<SlowLogEntry>, AppError>`。
- Produces `parse_slow_log_config_reply(value: redis::Value) -> Result<SlowLogConfig, AppError>`。
- Later tasks consume exact snake_case serde fields and `validate()` methods。

- [ ] **Step 1: 写失败的领域校验和解析测试**

在 `src-tauri/tests/domain.rs` 追加以下行为：

```rust
#[test]
fn rejects_invalid_observability_inputs_without_leaking_values() {
    let error = GetSlowLogsInput {
        connection_id: "".into(),
        count: 1001,
    }
    .validate()
    .unwrap_err();
    assert_eq!(error.code(), "INVALID_INPUT");
    assert_eq!(error.to_string(), "输入参数无效");

    let error = StartPubSubInput {
        connection_id: "local".into(),
        session_id: "session".into(),
        topics: vec![PubSubTopic {
            name: "   ".into(),
            pattern: false,
        }],
    }
    .validate()
    .unwrap_err();
    assert_eq!(error.code(), "INVALID_INPUT");
}

#[test]
fn normalizes_pubsub_topics_and_rejects_duplicates() {
    let input = StartPubSubInput {
        connection_id: "local".into(),
        session_id: "session".into(),
        topics: vec![
            PubSubTopic { name: " news.* ".into(), pattern: true },
            PubSubTopic { name: "news.*".into(), pattern: true },
        ],
    };
    assert_eq!(input.validate().unwrap_err().code(), "INVALID_INPUT");
}
```

在 `src-tauri/src/redis/observability.rs` 追加一个含 5/6 字段的 `SLOWLOG GET` RESP 数组测试，断言 id、duration、args、source、client；再追加不完整数组测试，断言只返回 `COMMAND_FAILED`。

运行：

```bash
cargo test --manifest-path src-tauri/Cargo.toml rejects_invalid_observability_inputs_without_leaking_values
```

Expected：FAIL，原因是 `InvalidInput`、DTO 或解析函数尚未定义。

- [ ] **Step 2: 写最小 DTO、错误码和纯解析实现**

在 `error.rs` 增加：

```rust
InvalidInput => ("INVALID_INPUT", "输入参数无效"),
OperationCancelled => ("OPERATION_CANCELLED", "操作已取消"),
```

在 `observability.rs` 实现字段校验：连接/session 非空，Slow Log count 为 `-1` 或 `1..=1000`，配置值满足 `max_len >= 0`、`slower_than >= -1` 且至少有一个字段，Pub/Sub 主题数量为 `1..=20` 且名称长度为 `1..=256`，发布 channel 非空且长度不超过 256。序列化使用 `#[derive(Serialize, Deserialize)]`，字段保持 Rust snake_case。

解析函数只接受 `Value::Array`/`Value::Attribute` 包裹的预期数组；通过现有 `Value` 标量转换 helper 读取整数和 UTF-8 文本，任何结构/编码/数字溢出都返回 `AppError::CommandFailed`，不把原始值放入错误。

- [ ] **Step 3: 运行聚焦测试确认 GREEN**

运行：

```bash
cargo test --manifest-path src-tauri/Cargo.toml observability
cargo test --manifest-path src-tauri/Cargo.toml --test domain
```

Expected：新增 DTO/解析测试通过，既有 domain 测试不回归。

- [ ] **Step 4: 格式化并提交**

```bash
cargo fmt --manifest-path src-tauri/Cargo.toml
git add src-tauri/src/domain/observability.rs src-tauri/src/domain/mod.rs src-tauri/src/error.rs src-tauri/src/redis/observability.rs src-tauri/src/redis/mod.rs src-tauri/tests/domain.rs
git commit -m "增加运维观察领域模型和解析"
```

### Task 2: Redis Slow Log 与 Pub/Sub service 生命周期

**Files:**
- Modify: `src-tauri/src/redis/connection_manager.rs`
- Modify: `src-tauri/src/redis/observability.rs`
- Modify: `src-tauri/Cargo.toml`, `src-tauri/Cargo.lock`
- Test: `src-tauri/src/redis/connection_manager.rs`, `src-tauri/tests/redis_integration.rs`

**Interfaces:**
- Consumes Task 1 DTO、校验和解析函数。
- Produces `RedisOperations::get_slow_logs`, `clear_slow_logs`, `get_slow_log_config`, `update_slow_log_config`, `publish_pub_sub`。
- Produces inherent `RedisService::start_pub_sub(app: tauri::AppHandle, input: StartPubSubInput) -> Result<PubSubSession, AppError>` and `stop_pub_sub(input: StopPubSubInput) -> Result<(), AppError>`。
- A connection has at most one active Pub/Sub task; stopping an absent session is idempotent。

- [ ] **Step 1: 先写 service RED 测试与真实 Redis ignored 流程**

在 `src-tauri/src/redis/connection_manager.rs` 的测试模块增加纯输入测试，证明不存在活动连接时 Slow Log 和 publish 返回 `CONNECTION_FAILED`。在 `src-tauri/tests/redis_integration.rs` 增加 `#[tokio::test] #[ignore] async fn runs_slow_log_and_pubsub_flow_when_redis_is_available()`，使用已有 `REDIX_TEST_REDIS_URL` profile，流程固定为：open → `clear_slow_logs` → `update_slow_log_config` → `get_slow_log_config` → publish → stop/close；不使用 SQL，不在循环中查询数据库。

运行：

```bash
cargo test --manifest-path src-tauri/Cargo.toml connection_manager::tests
cargo test --manifest-path src-tauri/Cargo.toml --test redis_integration runs_slow_log_and_pubsub_flow_when_redis_is_available
```

Expected：纯测试先因 trait 方法缺失 FAIL；ignored 测试会被编译但不执行。

- [ ] **Step 2: 添加最小 Redis service 方法**

在 `RedisService` 中增加一个 `Arc<Mutex<HashMap<String, PubSubTask>>>` 生命周期表，并在 `new` 中初始化；`PubSubTask` 保存 session id 和 `tokio::task::JoinHandle<()>`。在 `close_connection`、重新 `open_connection` 写入 client、`select_database` 替换 client 前调用按 connection id 的 cancel helper。

Slow Log 使用一个 multiplexed connection：

```rust
let reply: Value = redis::cmd("SLOWLOG")
    .arg("GET")
    .arg(count)
    .query_async(&mut connection)
    .await
    .map_err(map_command_error)?;
parse_slow_log_reply(reply)
```

`count == -1` 时先读取 config 的 max length，再用该值读取；`clear` 执行 `SLOWLOG RESET`；配置读取执行 `CONFIG GET slowlog-*`；配置更新只对非空字段执行 `CONFIG SET`，完成后重新读取并返回最终配置。

Pub/Sub 使用 `client.get_async_pubsub().await`，逐项执行 `subscribe`/`psubscribe`，然后 `tokio::spawn` 读取 `on_message()`。事件统一通过 `tauri::Emitter::emit` 发送，异常只映射为 `CONNECTION_FAILED`/`COMMAND_FAILED`。启动同一连接的新 session 前 abort 旧 handle；stop 从 map 移除并 abort，不存在时返回 `Ok(())`。

在 `Cargo.toml` 增加已锁定版本：

```toml
futures-util = { version = "0.3", default-features = false, features = ["std"] }
```

只导入 `futures_util::StreamExt` 读取 Pub/Sub stream；不新增网络服务或数据库依赖。

- [ ] **Step 3: 运行 service 聚焦测试并修复真实编译错误**

运行：

```bash
cargo fmt --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml connection_manager::tests
cargo test --manifest-path src-tauri/Cargo.toml --test redis_integration runs_slow_log_and_pubsub_flow_when_redis_is_available
```

Expected：无 Redis 时 ignored 测试显示 ignored；service 输入/错误测试通过。若编译错误来自 redis 1.5 的 `Value`/PubSub API，优先调整适配 helper，不改变 DTO 或命令名。

- [ ] **Step 4: 有 Redis 时验证完整流程**

```bash
REDIX_TEST_REDIS_URL=redis://127.0.0.1:6379 cargo test --manifest-path src-tauri/Cargo.toml --test redis_integration runs_slow_log_and_pubsub_flow_when_redis_is_available -- --ignored --nocapture
```

Expected：Slow Log 配置读写、清空、发布和订阅停止通过；没有 Redis 时记录 `CONNECTION_FAILED`，不伪造通过结果。

- [ ] **Step 5: 提交 service 变更**

```bash
git add src-tauri/src/redis/connection_manager.rs src-tauri/src/redis/observability.rs src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/tests/redis_integration.rs
git commit -m "实现 Slow Log 和 Pub Sub Redis 服务"
```

### Task 3: Tauri command 注册与 typed bridge

**Files:**
- Create: `src-tauri/src/commands/observability.rs`
- Modify: `src-tauri/src/commands/mod.rs`, `src-tauri/src/lib.rs`
- Modify: `src-tauri/tests/commands.rs`
- Modify: `src/lib/types.ts`, `src/lib/tauri.ts`
- Modify: `src/lib/tauri.test.ts`

**Interfaces:**
- Commands exactly match the spec: `get_slow_logs`, `clear_slow_logs`, `get_slow_log_config`, `update_slow_log_config`, `start_pub_sub`, `stop_pub_sub`, `publish_pub_sub`。
- Bridge exports `getSlowLogs`, `clearSlowLogs`, `getSlowLogConfig`, `updateSlowLogConfig`, `startPubSub`, `stopPubSub`, `publishPubSubMessage`, `listenPubSubMessages`, `listenPubSubStatus`。
- Listener helpers return `Promise<UnlistenFn>` and expose `PubSubMessageEvent`/`PubSubStatusEvent` payloads without `unknown` casts in pages.

- [ ] **Step 1: 写 command adapter 和 bridge RED 测试**

在 `src-tauri/tests/commands.rs` 的 `exposes_all_tauri_command_adapters` 增加 7 个函数指针断言；在 `src/lib/tauri.test.ts` 增加：

```ts
it("为运维观察命令使用稳定 snake_case 合同", async () => {
  const config = { slowlog_max_len: 128, slowlog_log_slower_than: 10000 };
  invokeMock.mockResolvedValueOnce([]).mockResolvedValueOnce(undefined)
    .mockResolvedValueOnce(config).mockResolvedValueOnce(config)
    .mockResolvedValueOnce({ connection_id: "local", session_id: "s1", topics: [] })
    .mockResolvedValueOnce(undefined).mockResolvedValueOnce(1);

  await expect(getSlowLogs({ connection_id: "local", count: 50 })).resolves.toEqual([]);
  expect(invokeMock).toHaveBeenLastCalledWith("get_slow_logs", {
    input: { connection_id: "local", count: 50 },
  });
  await getSlowLogConfig("local");
  expect(invokeMock).toHaveBeenLastCalledWith("get_slow_log_config", {
    connection_id: "local",
  });
  await publishPubSubMessage({ connection_id: "local", channel: "events", message: "ok" });
  expect(invokeMock).toHaveBeenLastCalledWith("publish_pub_sub", {
    input: { connection_id: "local", channel: "events", message: "ok" },
  });
});
```

将 `@tauri-apps/api/event` 的 `listen` mock 为 `vi.fn()`，先写两个 listener helper 断言事件名和返回 unlisten 函数。

运行：

```bash
npm test -- --run src/lib/tauri.test.ts
cargo test --manifest-path src-tauri/Cargo.toml --test commands exposes_all_tauri_command_adapters
```

Expected：前端因 wrapper/listener 不存在 FAIL；Rust 因 command module 未导出 FAIL。

- [ ] **Step 2: 实现 command adapter、注册和类型**

`commands/observability.rs` 只做输入转发和安全返回：

```rust
#[tauri::command(rename_all = "snake_case")]
pub async fn get_slow_logs(
    state: tauri::State<'_, AppState>,
    input: GetSlowLogsInput,
) -> Result<Vec<SlowLogEntry>, AppError> {
    state.redis.get_slow_logs(input).await
}
```

`start_pub_sub` 额外接收 `app: tauri::AppHandle`，调用 `state.redis.start_pub_sub(app, input).await`。在 `lib.rs` 的 `generate_handler!` 注册 7 个命令；在 `commands/mod.rs` re-export。

`src/lib/types.ts` 使用与 Rust 完全一致的 snake_case：`SlowLogEntry`、`SlowLogConfig`、`GetSlowLogsInput`、`UpdateSlowLogConfigInput`、`PubSubTopic`、`StartPubSubInput`、`PubSubSession`、`StopPubSubInput`、`PublishPubSubInput`、`PubSubMessageEvent`、`PubSubStatusEvent`。

`src/lib/tauri.ts` 继续复用 `call<T>`；listener helper 形状为：

```ts
export function listenPubSubMessages(
  handler: (event: PubSubMessageEvent) => void,
): Promise<UnlistenFn> {
  return listen<PubSubMessageEvent>(PUBSUB_MESSAGE_EVENT, (event) => handler(event.payload));
}
```

事件常量固定为 `redix://pubsub/message` 和 `redix://pubsub/status`。

- [ ] **Step 3: 运行 bridge/command GREEN 测试**

```bash
npm test -- --run src/lib/tauri.test.ts
cargo test --manifest-path src-tauri/Cargo.toml --test commands
```

Expected：bridge 新增断言和全部既有 bridge/command 测试通过；错误仍由 `call` 归一化为 code/message。

- [ ] **Step 4: 提交 IPC 合同**

```bash
git add src-tauri/src/commands/observability.rs src-tauri/src/commands/mod.rs src-tauri/src/lib.rs src-tauri/tests/commands.rs src/lib/types.ts src/lib/tauri.ts src/lib/tauri.test.ts
git commit -m "接入运维观察 Tauri 命令和桥接"
```

### Task 4: Slow Log React 状态与页面

**Files:**
- Create: `src/features/observability/slowLogState.ts`
- Create: `src/features/observability/slowLogState.test.ts`
- Create: `src/features/observability/SlowLogPage.tsx`
- Create: `src/features/observability/SlowLogPage.test.tsx`
- Consume: `src/lib/tauri.ts`, `src/lib/types.ts`

**Interfaces:**
- `normalizeSlowLogError(error: unknown): IpcError` maps `INVALID_INPUT`, `CONNECTION_FAILED`, `COMMAND_FAILED`, `IPC_ERROR` to fixed Chinese messages。
- `formatSlowLogArgs(args: string[]): string` joins arguments with a visible single space。
- `normalizeSlowLogCount(value: number): number` only returns `10|25|50|100|500`，非法值回退 `50`。
- `SlowLogPage({ connectionId }: { connectionId: string })` loads config and 50 entries on mount, refreshes on count change, and never updates state after unmount。

- [ ] **Step 1: 写 state 和页面 RED 测试**

在 state test 中先写：

```ts
it("把慢命令参数格式化为单行并保留空参数边界", () => {
  expect(formatSlowLogArgs(["SET", "key", "hello world"])).toBe("SET key hello world");
});

it("把 Redis 错误归一化为固定提示", () => {
  expect(normalizeSlowLogError({ code: "COMMAND_FAILED", message: "raw" })).toEqual({
    code: "COMMAND_FAILED",
    message: "Slow Log 操作失败，请检查 Redis 权限后重试。",
  });
});
```

页面 test mock `getSlowLogConfig`, `getSlowLogs`, `clearSlowLogs`, `updateSlowLogConfig`，断言页面标题、空状态、刷新按钮、数量选择、表格列和错误提示。先执行：

```bash
npm test -- --run src/features/observability/slowLogState.test.ts src/features/observability/SlowLogPage.test.tsx
```

Expected：FAIL，因为模块和页面尚不存在。

- [ ] **Step 2: 实现最小 state 与页面流程**

页面使用本地 state：`logs`、`config`、`count`、`loading`、`configSaving`、`clearing`、`error`、`notice`。用 `requestIdRef` 和 `mountedRef` 忽略旧请求；初次并行调用 config/logs，数量变化只刷新 logs。清空先调用 `window.confirm("确认清空 Redis Slow Log 记录吗？")`，取消不调用 IPC，成功后 `setLogs([])`。

页面结构固定为：

```tsx
<section className="observability-page slow-log-page" aria-labelledby="slow-log-page-title">
  <div className="page-heading">标题、说明、刷新/清空按钮</div>
  <div role="status" aria-live="polite">加载/成功提示</div>
  <p role="alert">固定错误提示</p>
  <section className="observability-panel">配置表单</section>
  <section className="observability-panel">数量选择与慢命令 table</section>
</section>
```

表格展示 ID、时间（本地化到秒）、耗时（`duration_us` + `μs`）、命令、来源、客户端；空列表显示“当前没有 Slow Log 记录，请刷新或检查 Redis 配置”。配置表单只提交两个合法数字，保存成功后刷新 config 和列表。

- [ ] **Step 3: 运行 Slow Log GREEN 测试并做 UI 修正**

```bash
npm test -- --run src/features/observability/slowLogState.test.ts src/features/observability/SlowLogPage.test.tsx
```

Expected：新增测试通过；页面在 `aria-busy`、错误提示、清空确认、配置保存 loading 期间不产生重复请求。

- [ ] **Step 4: 提交 Slow Log 页面**

```bash
git add src/features/observability/slowLogState.ts src/features/observability/slowLogState.test.ts src/features/observability/SlowLogPage.tsx src/features/observability/SlowLogPage.test.tsx
git commit -m "增加 Slow Log 工作区"
```

### Task 5: Pub/Sub 状态、事件监听与页面

**Files:**
- Create: `src/features/observability/pubSubState.ts`
- Create: `src/features/observability/pubSubState.test.ts`
- Create: `src/features/observability/PubSubPage.tsx`
- Create: `src/features/observability/PubSubPage.test.tsx`
- Consume: `src/lib/tauri.ts`, `src/lib/types.ts`

**Interfaces:**
- `parsePubSubTopics(raw: string, pattern: boolean): PubSubTopic[]` 按行解析、去空白、去重并保留模式标志。
- `appendPubSubMessage(messages: PubSubMessageEvent[], next: PubSubMessageEvent, activeSessionId: string, activeConnectionId: string): PubSubMessageEvent[]` 丢弃旧 session/connection 事件并截断到 5000 条。
- `normalizePubSubError(error: unknown): IpcError` 只返回固定提示。
- `PubSubPage({ connectionId }: { connectionId: string })` 管理一个当前 session，卸载时先 unlisten 再 stop。

- [ ] **Step 1: 写状态和页面 RED 测试**

```ts
const makeMessage = (
  index: number,
  session_id = "current",
  connection_id = "local",
): PubSubMessageEvent => ({
  connection_id,
  session_id,
  channel: "events",
  pattern: null,
  message: String(index),
  received_at_ms: index,
});

it("解析多行主题并在模式切换后生成 pattern topic", () => {
  expect(parsePubSubTopics("news.*\n logs.*\nnews.*", true)).toEqual([
    { name: "news.*", pattern: true },
    { name: "logs.*", pattern: true },
  ]);
});

it("丢弃旧 session 事件并把消息缓存限制为 5000 条", () => {
  const messages = Array.from({ length: 5000 }, (_, index) => makeMessage(index));
  const old = { ...makeMessage(5001), session_id: "old" };
  expect(appendPubSubMessage(messages, old, "current", "local")).toEqual(messages);
  const current = { ...makeMessage(5002), session_id: "current" };
  expect(appendPubSubMessage(messages, current, "current", "local")).toHaveLength(5000);
  expect(appendPubSubMessage(messages, current, "current", "local")[0].message).toBe("2");
});
```

页面 test mock `startPubSub`, `stopPubSub`, `publishPubSubMessage` 和两个 listener helper；断言订阅/取消/发布/清空、消息表、错误提示，以及 unmount 顺序。执行：

```bash
npm test -- --run src/features/observability/pubSubState.test.ts src/features/observability/PubSubPage.test.tsx
```

Expected：FAIL，因为 state/page 不存在。

- [ ] **Step 2: 实现最小 state 和生命周期**

页面 mount 时调用 `listenPubSubMessages`、`listenPubSubStatus`，保存两个 `UnlistenFn`；订阅按钮生成 `crypto.randomUUID()` session id，调用 `startPubSub`，成功后进入 subscribed 状态；取消或 cleanup 的顺序为 `await unlisten` → `stopPubSub`，其中 stop 失败只设置固定 notice。connection id 变化时旧 session 事件因 state filter 被丢弃。

订阅表单包含模式选择（频道/模式）、多行主题输入和订阅/取消按钮；发布表单包含 channel/message 和发布按钮；消息表展示 received time、channel、pattern、message，长文本使用 `overflow-wrap:anywhere`，并提供清空按钮。状态反馈使用 `role="status" aria-live="polite"`，错误使用 `role="alert"`。

- [ ] **Step 3: 运行 GREEN 测试并检查 cleanup**

```bash
npm test -- --run src/features/observability/pubSubState.test.ts src/features/observability/PubSubPage.test.tsx
```

Expected：状态 helper 和页面测试通过；`listen` 返回的 unlisten 在 stop 前执行，重复卸载不重复提交错误。

- [ ] **Step 4: 提交 Pub/Sub 页面**

```bash
git add src/features/observability/pubSubState.ts src/features/observability/pubSubState.test.ts src/features/observability/PubSubPage.tsx src/features/observability/PubSubPage.test.tsx
git commit -m "增加 Pub Sub 实时工作区"
```

### Task 6: 应用导航、样式、Smoke 与文档边界

**Files:**
- Modify: `src/App.tsx`
- Modify: `src/styles.css`
- Modify: `src/app.smoke.test.tsx`
- Modify: `README.md`
- Modify: `docs/non-cloud-scope.md`

**Interfaces:**
- Extends `Workspace` with `slow-log` and `pub-sub`。
- Navigation labels are `Slow Log`/`Pub/Sub`; both are disabled when no active profile。
- `App.tsx` passes the active profile id into both pages and renders only the active workspace。

- [ ] **Step 1: 写入口 RED 测试**

在 `src/app.smoke.test.tsx` 增加：

```tsx
it("未连接时禁用 Slow Log 和 Pub/Sub 导航", async () => {
  render(<App />);
  expect(screen.getByRole("button", { name: "Slow Log" })).toBeDisabled();
  expect(screen.getByRole("button", { name: "Pub/Sub" })).toBeDisabled();
});
```

先运行：

```bash
npm test -- --run src/app.smoke.test.tsx
```

Expected：FAIL，入口尚无两个导航项。

- [ ] **Step 2: 接入导航和工作区**

在 `Workspace` union、navigationItems、sectionDescriptions 和 `App.tsx` 的条件渲染中加入两个页面；为每个导航项使用现有 SVG stroke 图标风格。保留 query/settings 未连接可用策略，Slow Log/Pub/Sub 与 Browser/Workbench/Database 一样必须有 active profile。

- [ ] **Step 3: 添加紧凑可访问样式**

在 `styles.css` 新增 `.observability-page`、`.observability-panel`、`.observability-toolbar`、`.observability-table-wrap`、`.observability-table`、`.pubsub-message-table` 等类，复用现有 `--color-*` token。桌面使用双栏配置/消息区，窄屏在 `@media (max-width: 760px)` 改为单列；表格容器水平滚动但页面本身不产生横向溢出；输入最小高度 40px，焦点环和 disabled 状态与已有按钮一致。Slow Log 清空使用危险色，Pub/Sub 订阅使用主按钮，每个页面只保留一个主 CTA。

- [ ] **Step 4: 更新文档并运行入口测试**

README 增加 Slow Log 的四项能力和 Pub/Sub 的普通/模式订阅、发布、取消；当前边界写明不支持 Profiler、MONITOR、Cluster/Sentinel、TLS/SSH、Consumer Group 和模块专用能力。`docs/non-cloud-scope.md` 增加新增入口和排除词说明。

```bash
npm test -- --run src/app.smoke.test.tsx
npm run build
```

Expected：入口 smoke 与 TypeScript/Vite 构建通过。

- [ ] **Step 5: 提交应用集成**

```bash
git add src/App.tsx src/styles.css src/app.smoke.test.tsx README.md docs/non-cloud-scope.md
git commit -m "接入运维观察导航和应用样式"
```

### Task 7: 总体验收、真实 Redis 验证与交付审查

**Files:**
- Modify: `task_plan.md`, `findings.md`, `progress.md`
- Review: all files changed by Tasks 1-6

**Interfaces:**
- No new production interface; verifies the spec contract, tests, build, non-Cloud scan and lifecycle behavior together。

- [ ] **Step 1: 读取计划并核对工作区差异**

```bash
sed -n '1,260p' docs/superpowers/specs/2026-08-24-observability-design.md
sed -n '1,340p' docs/superpowers/plans/2026-08-24-observability.md
git status --short
git diff --check
```

逐项核对：Slow Log 四个命令、Pub/Sub 三个命令、两个事件名、固定错误码、连接清理、5000 条上限、无 Cloud/SQL 入口。

- [ ] **Step 2: 运行完整前端验证**

```bash
npm run test:frontend
npm run build
npm run check:non-cloud
```

Expected：全部前端测试通过、生产构建退出码为 0、非 Cloud 范围扫描通过。

- [ ] **Step 3: 运行完整 Rust 验证**

```bash
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo test --manifest-path src-tauri/Cargo.toml -q
```

Expected：格式检查通过，Rust 已执行测试全部通过；真实 Redis 集成测试未设置环境变量时保持 ignored。

- [ ] **Step 4: 运行真实 Redis Slow Log/Pub/Sub 流程**

```bash
REDIX_TEST_REDIS_URL=redis://127.0.0.1:6379 cargo test --manifest-path src-tauri/Cargo.toml --test redis_integration runs_slow_log_and_pubsub_flow_when_redis_is_available -- --ignored --nocapture
```

若 Redis 不可达，记录实际 `CONNECTION_FAILED` 环境限制，不将 ignored 或失败误报为通过；若可达，核对清空、配置、发布和取消订阅均完成。

- [ ] **Step 5: 做静态安全与范围审查**

```bash
rg -n -i 'redis.?cloud|azure|oauth|cloud.?api|telemetry|copilot|remote.?plugin|sql|keys\b' src src-tauri package.json
rg -n 'redix://pubsub/(message|status)|start_pub_sub|stop_pub_sub|publish_pub_sub' src src-tauri
git diff --check
```

确认新增产品源码没有 Cloud/Azure/AI/Telemetry/远程插件入口，没有 Browser `KEYS` 或 SQL 查询；确认 Pub/Sub 只从 active client 创建独立订阅 socket，所有页面 cleanup 都调用 unlisten 和 stop。

- [ ] **Step 6: 更新计划记录并提交收尾文档**

在 `task_plan.md` 追加 Phase 9 状态；在 `findings.md` 写入真实 Redis、测试数量和未实现能力；在 `progress.md` 记录每个任务 commit、失败/修复和最终验证结果。

```bash
git add task_plan.md findings.md progress.md
git commit -m "完成本地 Redis 运维观察能力验收"
```

## 计划自审

- Spec 的 Slow Log 四个 Redis 操作由 Task 2 service、Task 3 command、Task 4 page 覆盖。
- Spec 的 Pub/Sub 独立 socket、事件名、session 过滤、任务停止和连接切换清理由 Task 2、Task 3、Task 5 覆盖。
- Spec 的固定错误边界由 Task 1、Task 3、Task 4/5 覆盖。
- Spec 的导航、响应式、键盘/屏幕阅读器状态和文档边界由 Task 6 覆盖。
- Spec 的 Redis/前端/Rust/非 Cloud/差异检查由 Task 7 覆盖。
- 计划没有 `TBD`、`TODO`、未定义函数名或依赖未来任务才能运行的聚焦命令；每一项新增接口在前置任务的 Interfaces 中定义。
