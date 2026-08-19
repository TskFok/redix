# 数据库与实例概览实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 为当前 Standalone Redis 连接增加服务器/实例只读概览、数据库键空间概览和安全的数据库切换，并从应用导航进入独立的 Database 工作区。

**Architecture:** Rust 在 Redis 连接上集中执行 INFO、MODULE LIST、DBSIZE 和 SELECT，使用纯解析函数将 Redis 返回值转换成稳定 DTO；前端只消费 typed IPC，不自行解析 INFO。数据库切换先创建并 PING 新 database 的 Client，再原子保存 profile 并替换 active client；任何失败都保留旧 profile、旧 active client 和旧前端状态。

**Tech Stack:** Rust、Tokio、redis crate、serde/serde_json、Tauri 2、React、TypeScript、Vitest、Testing Library。

**Spec:** `docs/superpowers/specs/2026-08-18-redisinsight-non-cloud-parity-design.md`

## Global Constraints

- 当前 `main` 分支直接修改，不创建分支；提交信息使用简体中文。
- 只支持当前 Standalone Redis 连接模型，不加入 Redis Cloud、Azure、AI、Telemetry、远程插件、TLS/SSH/Sentinel/Cluster。
- 只执行只读概览命令和 SELECT；不调用 KEYS、不扫描全库计算统计、不在循环遍历中执行 SQL。
- INFO、MODULE LIST、指标权限不足或 Redis 版本不支持时按字段局部降级为 `null` 或空数组；不可将原始 Redis 错误文本传给前端。
- 数据库编号只允许 0 到 15，与现有 `ConnectionProfile.database` 校验保持一致；切换成功后才更新 profile 和 UI。
- 每个任务先写失败测试并确认 RED，再实现最小 GREEN；每个任务结束运行 `git diff --check`。

## 文件地图

- Create: `src-tauri/src/domain/database.rs` — 实例、数据库、切换 DTO 和 INFO 解析纯函数。
- Modify: `src-tauri/src/domain/mod.rs` — 导出数据库领域模型。
- Modify: `src-tauri/src/redis/connection_manager.rs` — 概览查询、模块列表解析和数据库 Client 切换。
- Create: `src-tauri/src/commands/database.rs` — Database Tauri command 适配器。
- Modify: `src-tauri/src/commands/mod.rs`、`src-tauri/src/lib.rs` — 模块导出和 command 注册。
- Modify: `src-tauri/tests/domain.rs`、`src-tauri/tests/commands.rs`、`src-tauri/tests/redis_integration.rs` — 纯解析、command 适配和真实 Redis 回归。
- Modify: `src/lib/types.ts`、`src/lib/tauri.ts`、`src/lib/tauri.test.ts` — 前端 DTO 与 typed wrapper。
- Create: `src/features/database/databaseState.ts`、`src/features/database/DatabasePage.tsx`、`src/features/database/database.test.tsx` — 概览状态、数据库选择和页面。
- Modify: `src/App.tsx`、`src/styles.css`、`src/app.smoke.test.tsx` — 导航入口、布局和应用回归。

### Task 1: 定义数据库领域模型和 INFO 解析器

**Files:**

- Create: `src-tauri/src/domain/database.rs`
- Modify: `src-tauri/src/domain/mod.rs`
- Test: `src-tauri/tests/domain.rs`

**Interfaces:**

~~~rust
pub struct InstanceOverview {
    pub server_version: Option<String>,
    pub redis_mode: Option<String>,
    pub uptime_seconds: Option<u64>,
    pub connected_clients: Option<u64>,
    pub used_memory_bytes: Option<u64>,
    pub max_memory_bytes: Option<u64>,
    pub total_commands_processed: Option<u64>,
    pub keyspace_hits: Option<u64>,
    pub keyspace_misses: Option<u64>,
    pub role: Option<String>,
    pub modules: Vec<ModuleSummary>,
}

pub struct ModuleSummary {
    pub name: String,
    pub version: Option<String>,
}

pub struct DatabaseOverview {
    pub database: u8,
    pub key_count: Option<u64>,
    pub expires: Option<u64>,
    pub avg_ttl_ms: Option<u64>,
}

pub struct SelectDatabaseInput {
    pub connection_id: String,
    pub database: u8,
}
~~~

实现 `parse_info_sections(raw: &str) -> HashMap<String, HashMap<String, String>>`、`parse_keyspace_line(database: &str, line: &str) -> Result<DatabaseOverview, AppError>` 和字段读取辅助函数。解析器必须忽略注释、空行和未知字段；数值溢出、非法数据库名或不合法 keyspace 项返回 `AppError::PersistenceFailed` 或 `AppError::InvalidConnection`，不得 panic。所有领域 DTO 派生 serde、Debug、Clone、PartialEq；切换输入实现 0 到 15 校验。

- [ ] **Step 1: Write the failing test**

在 `src-tauri/tests/domain.rs` 先增加：

~~~rust
#[test]
fn parses_info_sections_and_optional_metrics() {
    let sections = parse_info_sections(
        "# Server
redis_version:7.2.5
uptime_in_seconds:42

# Clients
connected_clients:3
# Keyspace
db0:keys=8,expires=2,avg_ttl=1200
",
    );
    assert_eq!(sections["Server"]["redis_version"], "7.2.5");
    assert_eq!(sections["Clients"]["connected_clients"], "3");
    let db = parse_keyspace_line("db0", "keys=8,expires=2,avg_ttl=1200").unwrap();
    assert_eq!(db.database, 0);
    assert_eq!(db.key_count, Some(8));
    assert_eq!(db.expires, Some(2));
    assert_eq!(db.avg_ttl_ms, Some(1200));
}

#[test]
fn rejects_invalid_database_selection_and_keyspace_lines() {
    assert_eq!(
        SelectDatabaseInput { connection_id: "local".into(), database: 16 }
            .validate()
            .unwrap_err(),
        AppError::InvalidConnection
    );
    assert_eq!(
        parse_keyspace_line("dbx", "keys=1,expires=0").unwrap_err(),
        AppError::PersistenceFailed
    );
}
~~~

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test domain parses_info_sections -q`

Expected: FAIL，因为数据库 DTO、解析函数和导出尚不存在。

- [ ] **Step 3: Write minimal implementation**

按 `# Section` 维护 section 名，按第一个冒号切分键和值，只保留非空键；`parse_keyspace_line` 解析 `dbN` 和逗号分隔的 `keys`、`expires`、`avg_ttl`。缺少可选指标时返回 `None`，但 `keys` 缺失或数据库编号非法必须返回固定错误。为 `InstanceOverview` 提供 `from_info_and_modules`，将 `used_memory`、`maxmemory` 等字段映射为字节数，将 `maxmemory:0` 视为 `None`。

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test domain parses_info_sections -q && cargo test --manifest-path src-tauri/Cargo.toml --test domain -q`

Expected: 新增解析测试和既有领域测试全部 PASS；不存在未处理的 panic。

- [ ] **Step 5: Commit**

~~~bash
git add src-tauri/src/domain/database.rs src-tauri/src/domain/mod.rs src-tauri/tests/domain.rs
git commit -m "增加数据库与实例概览领域模型"
~~~

### Task 2: 实现 Redis 概览查询、数据库切换和 Tauri commands

**Files:**

- Modify: `src-tauri/src/redis/connection_manager.rs`
- Create: `src-tauri/src/commands/database.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/lib.rs`
- Test: `src-tauri/tests/commands.rs`
- Test: `src-tauri/tests/redis_integration.rs`

**Interfaces:**

- 在 `RedisOperations` 增加 `get_instance_overview(connection_id: &str) -> Result<InstanceOverview, AppError>`。
- 在 `RedisOperations` 增加 `get_database_overview(connection_id: &str) -> Result<Vec<DatabaseOverview>, AppError>`。
- 在 `RedisOperations` 增加 `select_database(input: SelectDatabaseInput) -> Result<ConnectionProfile, AppError>`。
- 增加 `database::get_instance_overview`、`database::get_database_overview` 和 `database::select_database` 三个 `#[tauri::command]`，参数和返回值均为上述 typed DTO。

概览实现细节：

1. `get_instance_overview` 对同一个连接执行一次 `INFO`，并单独执行一次 `MODULE LIST`；权限不足或命令不支持时只让对应字段/模块列表降级，不影响其它 INFO 字段。
2. `get_database_overview` 执行一次 `INFO keyspace`，将每行 `dbN:keys=...,expires=...,avg_ttl=...` 解析成 DTO；若 INFO keyspace 不可用，再使用当前数据库的 `DBSIZE` 作为当前数据库的 `key_count`，其它字段为 `None`。不为了计算指标执行全库 SCAN。
3. `select_database` 读取 profile 和钥匙串密码，构造带目标 DB 编号的临时 Client，先调用 `PING`/现有 `inspect_client`；成功后保存新的 profile，再替换 active Client。保存失败时不替换 active Client；任何失败都返回固定错误并保留旧 profile、旧 active Client。
4. 抽出 `connection_url_with_database(profile, password, database)`，保持现有连接 URL 的密码边界；不把 URI 或密码放入 DTO、日志或错误。

- [ ] **Step 1: Write the failing test**

在 `src-tauri/tests/commands.rs` 的 command adapter 测试先引用：

~~~rust
let _ = database::get_instance_overview;
let _ = database::get_database_overview;
let _ = database::select_database;
~~~

在 `src-tauri/tests/redis_integration.rs` 增加真实 Redis 场景：向 DB 0 写入一个带 TTL 的 key，调用实例/数据库概览并断言 `server_version` 或 `key_count` 至少一项可用；调用 `select_database(database=1)` 后写入 DB 1，切回 DB 0，断言两个数据库数据隔离。增加 profile 持久化失败的 service 单元场景，断言旧 active client 仍可执行 PING，且旧 profile.database 不变。

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test commands exposes_all_tauri_command_adapters -q`

Expected: FAIL，因为数据库 command 和 RedisOperations 扩展尚不存在；集成测试暂时不能编译属于预期 RED。

- [ ] **Step 3: Write minimal implementation**

在 `connection_manager.rs` 使用单个连接顺序读取固定命令结果，再调用领域纯函数转换；不得把 `INFO` 原文直接返回前端。模块列表只保留 name/version，缺失 version 使用 `None`。数据库切换先 PING 新 Client，再调用现有 profile repository 保存，最后在 active map 中替换；为测试提供可注入 profile repository/secret store，避免在循环内查询任何外部数据。

- [ ] **Step 4: Run test to verify it passes**

Run:

~~~bash
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo test --manifest-path src-tauri/Cargo.toml --test commands --test domain -q
~~~

若本机 Redis 可用，再运行：

~~~bash
REDIX_TEST_REDIS_URL=redis://127.0.0.1:6379 cargo test --manifest-path src-tauri/Cargo.toml --test redis_integration -- --ignored --nocapture
~~~

Expected: command/domain/集成测试 PASS；无 Redis 或权限不足时，测试只接受固定降级 DTO/错误码，不吞掉真实失败。

- [ ] **Step 5: Commit**

~~~bash
git add src-tauri/src/redis/connection_manager.rs src-tauri/src/commands/database.rs src-tauri/src/commands/mod.rs src-tauri/src/lib.rs src-tauri/tests/commands.rs src-tauri/tests/redis_integration.rs
git commit -m "实现实例概览和数据库切换"
~~~

### Task 3: 实现 Database typed bridge 和概览页面

**Files:**

- Modify: `src/lib/types.ts`
- Modify: `src/lib/tauri.ts`
- Modify: `src/lib/tauri.test.ts`
- Create: `src/features/database/databaseState.ts`
- Create: `src/features/database/DatabasePage.tsx`
- Create: `src/features/database/database.test.tsx`

**Interfaces:**

- `getInstanceOverview(connectionId): Promise<InstanceOverview>`
- `getDatabaseOverview(connectionId): Promise<DatabaseOverview[]>`
- `selectDatabase(input: SelectDatabaseInput): Promise<ConnectionProfile>`
- `DatabasePageProps { connectionId: string; activeDatabase: number; onProfileChanged(profile: ConnectionProfile): void }`
- `DatabasePageState { loading; switching; instance; databases; error; switchError; requestId }`

- [ ] **Step 1: Write the failing test**

在 `src/lib/tauri.test.ts` 断言三个 wrapper 使用命令名 `get_instance_overview`、`get_database_overview`、`select_database`，并且 payload 分别为 `{ connection_id }` 和 `{ input }`。

在 `database.test.tsx` 增加：

- 加载完成后渲染服务器版本、内存/客户端指标，以及数据库列表。
- `null` 指标显示“不可用”，不把 `undefined` 渲染为字符串。
- 数据库切换调用一次 `selectDatabase`；成功后调用 `onProfileChanged`，失败后保留原选中数据库并显示固定错误。
- 组件卸载或连接 ID 改变后忽略旧的概览响应。

- [ ] **Step 2: Run test to verify it fails**

Run: `npm test -- --run src/lib/tauri.test.ts src/features/database/database.test.tsx`

Expected: FAIL，因为 bridge、state 和 Database 页面尚不存在。

- [ ] **Step 3: Write minimal implementation**

`databaseState.ts` 提供初始状态、request ID 判断、`formatMetric` 和固定错误映射。页面挂载时用 `Promise.allSettled` 并行请求实例/数据库概览，成功结果分别写入状态；一个请求失败不清除另一个成功结果。切换按钮在请求期间禁用，成功只通过 `onProfileChanged` 更新 App 的 active profile；页面自身不乐观修改 active database。

指标卡显示 `server_version`、`redis_mode`、`connected_clients`、`used_memory_bytes`、`keyspace_hits`、`keyspace_misses`；数据库表显示 DB 编号、键数、过期数、平均 TTL。所有 null 值显示固定“不可用”，不显示 Redis 原始错误。

- [ ] **Step 4: Run test to verify it passes**

Run: `npm test -- --run src/lib/tauri.test.ts src/features/database/database.test.tsx && npm run build`

Expected: Database focused tests、bridge tests 和生产构建 PASS。

- [ ] **Step 5: Commit**

~~~bash
git add src/lib/types.ts src/lib/tauri.ts src/lib/tauri.test.ts src/features/database/databaseState.ts src/features/database/DatabasePage.tsx src/features/database/database.test.tsx
git commit -m "增加数据库概览页面和 typed bridge"
~~~

### Task 4: 接入应用导航并保持连接回归

**Files:**

- Modify: `src/lib/types.ts`
- Modify: `src/App.tsx`
- Modify: `src/styles.css`
- Modify: `src/app.smoke.test.tsx`

**Interfaces:**

- 将 `Workspace` 扩展为 `"browser" | "workbench" | "database"`。
- `AppSection` 保持连接管理与工作区联合类型；Database、Browser、Workbench 在无 active profile 时都不可点击。
- `handleOpenConnection`、`onProfileChanged` 必须保持 active profile 的 id/name/host/port/database 一致；Database 切换成功后回到 Database 页面并刷新其它工作区使用的 profile。

- [ ] **Step 1: Write the failing test**

在 `src/app.smoke.test.tsx` 增加：

~~~tsx
it("连接后显示 Database 工作区，未连接时禁用", () => {
  render(<App />);
  expect(screen.getByRole("button", { name: "Database" })).toBeDisabled();
  // 使用现有连接 mock 打开后：
  expect(screen.getByRole("button", { name: "Database" })).toBeEnabled();
  fireEvent.click(screen.getByRole("button", { name: "Database" }));
  expect(screen.getByRole("heading", { name: "数据库概览" })).toBeInTheDocument();
});
~~~

测试使用现有 `ConnectionPage` mock/fixture，不新建第二套连接状态。

- [ ] **Step 2: Run test to verify it fails**

Run: `npm test -- --run src/app.smoke.test.tsx`

Expected: FAIL，因为 Workspace 没有 Database 入口，App 也未渲染 DatabasePage。

- [ ] **Step 3: Write minimal implementation**

增加 Database 导航图标和描述；从 `App` 传入当前 profile 的 `database`，并在 `onProfileChanged` 中只替换当前 profile，保留连接页面和 Browser/Workbench 的已有行为。Database 页面需要的刷新/切换成功回调不触发额外的 open/close connection。

- [ ] **Step 4: Run test to verify it passes**

Run:

~~~bash
npm test -- --run src/app.smoke.test.tsx src/features/connections/connections.test.tsx
npm run test:frontend
npm run build
git diff --check
~~~

Expected: Database 导航、连接管理和现有工作区回归全部 PASS。

- [ ] **Step 5: Commit**

~~~bash
git add src/lib/types.ts src/App.tsx src/styles.css src/app.smoke.test.tsx
git commit -m "接入数据库概览应用导航"
~~~

### Task 5: 数据库子计划范围审查和文档记录

**Files:**

- Modify: `README.md`
- Modify: `docs/non-cloud-scope.md`
- Modify: `progress.md`
- Modify: `task_plan.md`

- [ ] **Step 1: Write the failing regression test**

在应用 smoke 测试增加只读边界回归：Database 页面显示“刷新概览”和数据库切换，不显示 Cloud/Azure/AI/Telemetry/远程插件入口；无 active profile 时所有 Redis 工作区保持禁用。

- [ ] **Step 2: Run test to verify it fails**

Run: `npm test -- --run src/app.smoke.test.tsx`

Expected: FAIL，因为范围说明和 Database 入口尚未接入。

- [ ] **Step 3: Write minimal implementation**

更新 README 与 `docs/non-cloud-scope.md`，明确 Database 只读概览使用 INFO/DBSIZE、切换只针对 Standalone 数据库编号；把真实 Redis 可用性、权限降级和测试结果写入 `progress.md`，同步 `task_plan.md` 的 Phase 8 状态。

- [ ] **Step 4: Run test to verify it passes**

Run:

~~~bash
npm run check:non-cloud
npm run test:frontend
npm run build
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo test --manifest-path src-tauri/Cargo.toml
git diff --check
~~~

Expected: 非 Cloud 扫描、前端、构建、Rust 和差异检查全部 PASS。

- [ ] **Step 5: Commit**

~~~bash
git add README.md docs/non-cloud-scope.md progress.md task_plan.md
git commit -m "记录数据库概览范围和验证结果"
~~~
