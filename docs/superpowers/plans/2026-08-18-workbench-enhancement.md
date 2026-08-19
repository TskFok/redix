# Workbench 增强实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在现有单条命令 Workbench 上增加本地命令目录、轻量参数提示、多命令执行、停止策略、raw/text/JSON 结果、复制和按连接持久化的非敏感命令历史。

**Architecture:** 命令目录作为 Rust 内置静态 DTO 返回，前端负责输入提示。多命令通过一个 `execute_commands` IPC 请求交给 Rust 顺序执行，Rust 返回逐条结果；历史使用子计划 1 提供的 `JsonDocumentStore` 写入本地 JSON，并通过 `AppState` 的数据目录注入。

**Tech Stack:** Rust、Tokio、redis crate、serde/serde_json、Tauri 2、React、TypeScript、Vitest、Testing Library。

**Spec:** `docs/superpowers/specs/2026-08-18-redisinsight-non-cloud-parity-design.md`

## Global Constraints

- 必须先读取并使用 `2026-08-18-browser-productivity.md` 产生的 `JsonDocumentStore`；不复制第二套原子 JSON 写入逻辑。
- 不使用 Monaco、插件运行时或云端命令目录；命令目录由 Rust 内置静态数据提供。
- 多命令由一个 Rust IPC 执行，不在前端循环调用 `executeCommand`。
- `AUTH`、`HELLO`、敏感 `ACL`/`CONFIG` 命令不进入历史；历史不保存 URI、密码、底层错误文本。
- 继续使用固定 `AppError`/`IpcError` 映射，提交信息使用简体中文，当前 `main` 分支直接修改。
- 每个任务先 RED 再 GREEN；每次任务结束运行 `git diff --check`。

## 文件地图

- Modify: `src-tauri/src/domain/workbench.rs` — 命令目录、批量执行、历史 DTO 和敏感命令识别。
- Modify: `src-tauri/src/redis/connection_manager.rs`、`src-tauri/src/redis/workbench.rs` — 批量执行和安全结果转换。
- Modify: `src-tauri/src/commands/workbench.rs`、`src-tauri/src/commands/mod.rs`、`src-tauri/src/lib.rs` — Workbench commands 注册。
- Modify: `src-tauri/src/lib.rs` — 为 history JSON 注入应用数据目录。
- Modify: `src-tauri/src/persistence/document_store.rs` — 使用 Browser 子计划提供的 load/save 和损坏文件保护。
- Modify: `src-tauri/tests/domain.rs`、`src-tauri/tests/commands.rs`、`src-tauri/tests/redis_integration.rs`、`src-tauri/tests/persistence.rs` — Rust 行为测试、历史文档版本化读写和连接隔离。
- Modify: `src/lib/types.ts`、`src/lib/tauri.ts`、`src/lib/tauri.test.ts` — typed batch/catalog/history bridge。
- Modify: `src/features/workbench/workbenchState.ts`、`WorkbenchPage.tsx`、`CommandInput.tsx`、`CommandResult.tsx`。
- Create: `src/features/workbench/CommandSuggestions.tsx`、`src/features/workbench/workbenchHistory.ts`。
- Modify: `src/features/workbench/workbench.test.tsx`、`src/app.smoke.test.tsx`。

### Task 1: 定义命令目录、批量结果和敏感历史规则

**Files:**

- Modify: `src-tauri/src/domain/workbench.rs`
- Modify: `src-tauri/src/domain/mod.rs`
- Modify: `src-tauri/tests/domain.rs`

**Interfaces:**

```rust
pub struct CommandArgument {
    pub name: String,
    pub required: bool,
    pub hint: String,
}

pub struct CommandDefinition {
    pub name: String,
    pub summary: String,
    pub arguments: Vec<CommandArgument>,
}

pub struct ExecuteCommandsInput {
    pub connection_id: String,
    pub commands: Vec<String>,
    pub continue_on_error: bool,
}

pub struct CommandExecutionItem {
    pub command: String,
    pub result: Option<CommandResult>,
    pub error_code: Option<String>,
}

pub struct CommandHistoryEntry {
    pub connection_id: String,
    pub command: String,
    pub result: Option<CommandResult>,
    pub error_code: Option<String>,
    pub created_at: String,
}

pub struct CommandHistoryDocument {
    pub version: u32,
    pub entries: Vec<CommandHistoryEntry>,
}
```

Producing functions: `command_catalog() -> Vec<CommandDefinition>`, `is_sensitive_command(&str) -> bool`, `filter_history_entry(&CommandHistoryEntry) -> Option<CommandHistoryEntry>`. `CommandHistoryDocument` implements the versioned JSON document trait from the Browser subplan with version 1 and an empty default document.

- [ ] **Step 1: Write the failing test**

在 `src-tauri/tests/domain.rs` 增加：

```rust
#[test]
fn command_catalog_contains_safe_high_frequency_commands() {
    let names = command_catalog().into_iter().map(|item| item.name).collect::<Vec<_>>();
    assert!(names.contains(&"PING".into()));
    assert!(names.contains(&"GET".into()));
    assert!(names.contains(&"SET".into()));
}

#[test]
fn sensitive_commands_are_not_saved_to_history() {
    assert!(is_sensitive_command("AUTH secret"));
    assert!(is_sensitive_command("CONFIG SET requirepass secret"));
    assert!(is_sensitive_command("ACL SETUSER alice on >secret"));
    assert!(!is_sensitive_command("GET user:1"));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test domain command_catalog -q`

Expected: FAIL，因为新 DTO、catalog 和敏感命令函数不存在。

- [ ] **Step 3: Write minimal implementation**

在 `domain/workbench.rs` 增加 serde DTO、空命令/空 connection 校验、固定内置目录（至少包含 `PING`、`GET`、`SET`、`DEL`、`EXISTS`、`TYPE`、`TTL`、`PTTL`、`DBSIZE`、`INFO`、`SCAN`）和大小写不敏感的敏感命令判断。历史过滤按第一个 token 判断命令名，并对 `AUTH`、`HELLO`、`ACL`、`CONFIG` 整个命令族拒绝保存。

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test domain command_catalog -q`

Expected: 新增 domain tests PASS，既有 tokenizer/domain tests PASS。

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/domain/workbench.rs src-tauri/src/domain/mod.rs src-tauri/tests/domain.rs
git commit -m "增加 Workbench 命令目录和敏感命令过滤"
```

### Task 2: 实现 Rust 多命令执行和 commands

**Files:**

- Modify: `src-tauri/src/redis/connection_manager.rs`
- Modify: `src-tauri/src/redis/workbench.rs`
- Modify: `src-tauri/src/commands/workbench.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/tests/commands.rs`
- Modify: `src-tauri/tests/redis_integration.rs`
- Modify: `src-tauri/tests/persistence.rs`

**Interfaces:**

- Extend `RedisOperations` with `execute_commands(&self, input: ExecuteCommandsInput) -> Result<Vec<CommandExecutionItem>, AppError>` and `command_catalog(&self) -> Vec<CommandDefinition>`。
- Add Tauri commands `execute_commands` and `get_command_catalog`。
- Add `SaveCommandHistoryInput { connection_id: String, entries: Vec<CommandHistoryEntry> }`, plus `list_command_history(connection_id: String) -> Result<Vec<CommandHistoryEntry>, AppError>` and `save_command_history(input: SaveCommandHistoryInput) -> Result<(), AppError>` commands; the save input contains one connection id and a batch of entries.
- Extend `AppState` with `data_dir: PathBuf`; keep `AppState::new(profiles, secrets)` for existing tests and add `AppState::with_data_dir(profiles, secrets, data_dir)` for the runtime and isolated resource tests.
- Existing `execute_command` remains backward-compatible and delegates to the same tokenization/result conversion helper where possible。

- [ ] **Step 1: Write the failing test**

在 `src-tauri/tests/commands.rs` 增加 command adapter 引用：

```rust
let _ = workbench::execute_commands;
let _ = workbench::get_command_catalog;
let _ = workbench::list_command_history;
let _ = workbench::save_command_history;
```

在 `redis_integration.rs` 增加 `PING`, `DBSIZE`, invalid command 的 batch 场景：默认错误停止，`continue_on_error=true` 时保留后续结果。

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test commands exposes_all_tauri_command_adapters -q`

Expected: FAIL，因为 batch command 尚未注册。

- [ ] **Step 3: Write minimal implementation**

在 `RedisService` 内把命令列表按输入顺序逐个 tokenize 和执行，构造 `CommandExecutionItem`；`continue_on_error=false` 时遇到第一个错误就停止并保留此前结果，`true` 时继续。每条命令的 `AppError` 只转换成 `error_code`，不写入底层 message。空列表、超过 100 条、空命令在 DTO 校验阶段返回 `InvalidConnection`。

`get_command_catalog` 只返回内置静态目录；不读取网络、不读取 Redis Cloud API。历史 commands 使用 `JsonDocumentStore`，路径为 `state.data_dir.join("workbench-history.json")`：保存前调用 `filter_history_entry`，按 `connection_id` 隔离并限制为最近 100 条；list 只返回请求 connection id 的条目。损坏 JSON 使用文档仓储的默认恢复能力但不覆盖原文件，权限/目录错误返回 `PersistenceFailed`。更新 `commands/mod.rs` re-export 和 `src-tauri/src/lib.rs` 的 `generate_handler!`，并在 `AppState::with_data_dir` 测试中使用临时目录。

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check && cargo test --manifest-path src-tauri/Cargo.toml --test commands --test domain -q`

Expected: command/domain tests PASS。

若本机 Redis 可用，运行：

```bash
REDIX_TEST_REDIS_URL=redis://127.0.0.1:6379 cargo test --manifest-path src-tauri/Cargo.toml --test redis_integration -- --ignored --nocapture
```

Expected: batch stop/continue/result 顺序 PASS。

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/redis/connection_manager.rs src-tauri/src/redis/workbench.rs src-tauri/src/commands/workbench.rs src-tauri/src/commands/mod.rs src-tauri/src/lib.rs src-tauri/tests/commands.rs src-tauri/tests/persistence.rs src-tauri/tests/redis_integration.rs
git commit -m "实现 Workbench 多命令执行"
```

### Task 3: 实现 Workbench bridge、提示、结果格式和安全历史

**Files:**

- Modify: `src/lib/types.ts`
- Modify: `src/lib/tauri.ts`
- Modify: `src/lib/tauri.test.ts`
- Create: `src/features/workbench/CommandSuggestions.tsx`
- Create: `src/features/workbench/workbenchHistory.ts`
- Modify: `src/features/workbench/workbenchState.ts`
- Modify: `src/features/workbench/WorkbenchPage.tsx`
- Modify: `src/features/workbench/CommandInput.tsx`
- Modify: `src/features/workbench/CommandResult.tsx`
- Modify: `src/features/workbench/workbench.test.tsx`

**Interfaces:**

- `executeCommands(input: ExecuteCommandsInput): Promise<CommandExecutionItem[]>`。
- `getCommandCatalog(): Promise<CommandDefinition[]>`。
- `listCommandHistory(connectionId: string): Promise<CommandHistoryEntry[]>`。
- `saveCommandHistory(input: SaveCommandHistoryInput): Promise<void>`。
- `CommandDisplayFormat = "raw" | "text" | "json"`。
- `WorkbenchPageState` 增加 `commands: string[]`、`continueOnError`、`format`、`batchResults`、`catalog` 和 `catalogLoading`。
- `filterHistoryEntry(command: string): boolean` 必须与 Rust 敏感命令规则保持相同命令族。

- [ ] **Step 1: Write the failing test**

在 `src/lib/tauri.test.ts` 断言两个 wrapper 的 invoke 名和 `{ input }` payload。在 `workbench.test.tsx` 增加：

```tsx
it("一次 IPC 执行多条命令并按顺序展示结果", async () => {
  executeCommandsMock.mockResolvedValue([
    { command: "PING", result: { kind: "string", value: "PONG" }, error_code: null },
    { command: "DBSIZE", result: { kind: "number", value: 2 }, error_code: null },
  ]);
  render(<WorkbenchPage connectionId="local" />);
  fireEvent.change(screen.getByRole("textbox", { name: "Redis 命令" }), {
    target: { value: "PING\nDBSIZE" },
  });
  fireEvent.click(screen.getByRole("button", { name: "执行" }));

  await screen.findByText("PONG");
  expect(executeCommandsMock).toHaveBeenCalledTimes(1);
  expect(executeCommandsMock).toHaveBeenCalledWith({
    input: { connection_id: "local", commands: ["PING", "DBSIZE"], continue_on_error: false },
  });
});
```

另加 raw/text/JSON 切换、复制按钮、提示目录过滤、`listCommandHistory` 按连接隔离、执行后只调用一次 `saveCommandHistory`，以及 `AUTH secret` 不进入历史的测试。

- [ ] **Step 2: Run test to verify it fails**

Run: `npm test -- --run src/lib/tauri.test.ts src/features/workbench/workbench.test.tsx`

Expected: FAIL，因为新 bridge、批量 state 和结果组件尚不存在。

- [ ] **Step 3: Write minimal implementation**

将文本框按换行拆成非空命令数组，单次调用 `executeCommands`；保留现有 Cmd/Ctrl+Enter 行为。`CommandSuggestions` 根据 catalog 的 command 前缀和当前 token 展示可访问列表，键盘上下键选择、Enter 回填但不自动执行。
页面挂载时一次加载命令目录和当前连接历史，连接变化时重新加载历史并忽略过期响应。

`CommandResult` 根据 `format`：raw 使用纯文本/字符串，text 使用可读缩进，json 使用 `JSON.stringify(value, null, 2)`；复制使用 `navigator.clipboard.writeText`，失败显示固定提示。批量结果逐项展示 command、状态、结果或稳定错误码。

`workbenchHistory.ts` 只负责把执行结果映射为 `CommandHistoryEntry`、调用 `saveCommandHistory` 和按时间排序展示；历史实际由 Rust 的 `JsonDocumentStore` 写入 `workbench-history.json`。前端在保存前按同构规则隐藏敏感命令，Rust 再做最终过滤；按 `connection_id` 隔离，最多保留最近 100 条，不保存密码和底层错误 message。不得调用 `localStorage`。

- [ ] **Step 4: Run test to verify it passes**

Run: `npm test -- --run src/lib/tauri.test.ts src/features/workbench/workbench.test.tsx && npm run build`

Expected: Workbench focused tests、历史连接隔离测试和生产构建 PASS；既有单命令回归仍通过。

- [ ] **Step 5: Commit**

```bash
git add src/lib/types.ts src/lib/tauri.ts src/lib/tauri.test.ts src/features/workbench/CommandSuggestions.tsx src/features/workbench/workbenchHistory.ts src/features/workbench/workbenchState.ts src/features/workbench/WorkbenchPage.tsx src/features/workbench/CommandInput.tsx src/features/workbench/CommandResult.tsx src/features/workbench/workbench.test.tsx
git commit -m "增强 Workbench 提示结果和安全历史"
```

### Task 4: Workbench 子计划验证和文档

**Files:**

- Modify: `README.md`
- Modify: `docs/non-cloud-scope.md`
- Modify: `progress.md`
- Modify: `task_plan.md`

- [ ] **Step 1: Write the failing regression test**

在 `app.smoke.test.tsx` 增加 Workbench 增强入口回归：已连接时显示命令目录提示、批量错误策略和结果格式控件；未连接时仍禁用 Workbench。

- [ ] **Step 2: Run test to verify it fails**

Run: `npm test -- --run src/app.smoke.test.tsx`

Expected: FAIL，因为 App/Workbench 尚未提供新控件。

- [ ] **Step 3: Write minimal implementation**

更新应用说明和非 Cloud 范围文档，明确命令目录是本地内置数据、历史不保存敏感命令、未实现 Monaco/插件可视化；将真实测试和任何 Redis 环境限制记录到 `progress.md`。

- [ ] **Step 4: Run test to verify it passes**

Run: `npm run check:non-cloud && npm run test:frontend && npm run build && cargo fmt --manifest-path src-tauri/Cargo.toml -- --check && cargo test --manifest-path src-tauri/Cargo.toml && git diff --check`

Expected: 全部通过，且源码扫描不出现 Cloud/Azure/AI/Telemetry/远程插件入口。

- [ ] **Step 5: Commit**

```bash
git add README.md docs/non-cloud-scope.md progress.md task_plan.md
git commit -m "记录 Workbench 增强范围和验证结果"
```
