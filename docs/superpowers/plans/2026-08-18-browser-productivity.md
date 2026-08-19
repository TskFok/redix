# Browser 生产力增强实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在现有 Browser 上补齐键名搜索、Redis 类型过滤、刷新、可选元数据和受校验的本地 JSON 导入导出，并建立后续子计划复用的版本化 JSON 文档仓储。

**Architecture:** Rust 继续通过 `RedisOperations` 访问当前连接；`SCAN` 保持增量 cursor，类型过滤在扫描过程中按 `KeySummary.key_type` 过滤。前端用现有 Browser state/request token 管理筛选和过期响应；文件由原生 file input/Blob 处理，Redis 写入通过一次 typed IPC。

**Tech Stack:** Rust、Tokio、redis crate、serde/serde_json、Tauri 2、React、TypeScript、Vitest、Testing Library。

**Spec:** `docs/superpowers/specs/2026-08-18-redisinsight-non-cloud-parity-design.md`

## Global Constraints

- 当前 `main` 分支直接修改，不创建分支；提交信息使用简体中文。
- Browser 只能使用 `SCAN`，不使用 `KEYS`；不在循环遍历中执行 SQL。
- 导入文件只解析受支持 JSON DTO，不执行文件中的命令或脚本；导入默认禁止覆盖已有键。
- 密码、URI 和 Redis 底层错误文本不进入 DTO、JSON、历史或 UI。
- RedisJSON/其他模块不可用时返回固定 `UNSUPPORTED_DATA_TYPE`，不让基础 Browser 失败。
- 先写失败测试并确认 RED，再写生产代码；每个任务都运行 `git diff --check`。

## 文件地图

- Create: `src-tauri/src/persistence/document_store.rs` — 通用版本化 JSON 文档读写和原子替换。
- Modify: `src-tauri/src/persistence/mod.rs` — 导出文档仓储。
- Modify: `src-tauri/src/domain/key.rs` — 扩展 scan/filter/import/export DTO 和校验。
- Modify: `src-tauri/src/domain/mod.rs` — re-export 新模型。
- Modify: `src-tauri/src/redis/connection_manager.rs` — 类型过滤、列表元数据、导入导出 service 方法。
- Modify: `src-tauri/src/commands/browser.rs`、`src-tauri/src/commands/mod.rs`、`src-tauri/src/lib.rs` — Tauri command 适配和注册。
- Modify: `src-tauri/tests/domain.rs`、`src-tauri/tests/persistence.rs`、`src-tauri/tests/commands.rs`、`src-tauri/tests/redis_integration.rs` — Rust 行为和集成测试。
- Modify: `src/lib/types.ts`、`src/lib/tauri.ts`、`src/lib/tauri.test.ts` — 前端 DTO 和 IPC wrapper。
- Modify: `src/features/browser/browserState.ts`、`src/features/browser/BrowserPage.tsx`、`KeyList.tsx`、`KeyDetails.tsx`、`browserState.test.ts`、`browser.test.tsx` — 筛选、元数据和导入导出 UI。
- Create: `src/features/browser/BrowserImportExport.tsx` — 原生 file input、Blob 导出和导入 DTO 解析。

### Task 1: 建立版本化 JSON 文档仓储

**Files:**

- Create: `src-tauri/src/persistence/document_store.rs`
- Modify: `src-tauri/src/persistence/mod.rs`
- Test: `src-tauri/tests/persistence.rs`

**Interfaces:**

- Produces `VersionedJsonDocument` trait、`JsonDocumentStore::new(path)`、`JsonDocumentStore::load<T>()`、`JsonDocumentStore::load_or_default<T>()` 和 `JsonDocumentStore::save<T>()`，供 Workbench/Query Library/Settings 子计划使用。
- `load<T>()` 对格式/迁移错误返回 `AppError::PersistenceFailed`；`load_or_default<T>()` 只把 JSON 解析/迁移错误降级为 `T::default()`，保留原文件不写回，文件系统权限/读取错误仍返回 `PersistenceFailed`；缺失文件返回 `T::default()`。

- [ ] **Step 1: Write the failing test**

在 `src-tauri/tests/persistence.rs` 增加以下行为测试：

```rust
#[test]
fn versioned_document_round_trips_and_replaces_atomically() {
    let directory = temporary_directory("document-round-trip");
    let path = directory.join("settings.json");
    let store = JsonDocumentStore::new(path.clone());

    store.save(&TestDocument::default()).unwrap();
    assert_eq!(store.load::<TestDocument>().unwrap(), TestDocument::default());
    assert_eq!(fs::read_dir(&directory).unwrap().count(), 1);
    remove_temporary_directory(&directory);
}

#[test]
fn malformed_versioned_document_returns_safe_persistence_error() {
    let directory = temporary_directory("document-malformed");
    let path = directory.join("settings.json");
    fs::write(&path, "{ invalid json").unwrap();

    let error = JsonDocumentStore::new(path).load::<TestDocument>().unwrap_err();
    assert_eq!(error, AppError::PersistenceFailed);
    remove_temporary_directory(&directory);
}
```

测试在同一文件声明 `TestDocument`、`Default + Serialize + Deserialize + PartialEq` 派生和 `VersionedJsonDocument` 实现，`migrate` 使用 `serde_json::from_value` 并把失败映射为 `PersistenceFailed`；不要依赖 Redis。

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test persistence versioned_document -q`

Expected: FAIL，因为 `JsonDocumentStore`、`VersionedJsonDocument` 和测试文档尚不存在。

- [ ] **Step 3: Write minimal implementation**

在 `document_store.rs` 实现以下结构和行为：

```rust
pub trait VersionedJsonDocument:
    serde::Serialize + serde::de::DeserializeOwned + Default + PartialEq
{
    fn version(&self) -> u32;
    fn migrate(value: serde_json::Value) -> Result<Self, AppError>;
}

pub struct JsonDocumentStore {
    pub path: PathBuf,
}

impl JsonDocumentStore {
    pub fn new(path: PathBuf) -> Self;
    pub fn load<T: VersionedJsonDocument>(&self) -> Result<T, AppError>;
    pub fn load_or_default<T: VersionedJsonDocument>(&self) -> Result<T, AppError>;
    pub fn save<T: VersionedJsonDocument>(&self, document: &T) -> Result<(), AppError>;
}
```

沿用 `profile_store.rs` 的临时文件、父目录创建、`fs::rename`/Windows 替换和临时文件清理逻辑；JSON 解析、序列化、迁移失败统一返回 `PersistenceFailed`。

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test persistence versioned_document -q`

Expected: 2 个版本化文档测试 PASS，并且既有 persistence 测试不回归。

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/persistence/document_store.rs src-tauri/src/persistence/mod.rs src-tauri/tests/persistence.rs
git commit -m "增加版本化 JSON 文档仓储"
```

### Task 2: 扩展 Browser 领域 DTO 和 IPC 合同

**Files:**

- Modify: `src-tauri/src/domain/key.rs`
- Modify: `src-tauri/src/domain/mod.rs`
- Modify: `src/lib/types.ts`
- Modify: `src/lib/tauri.ts`
- Test: `src-tauri/tests/domain.rs`、`src/lib/tauri.test.ts`

**Interfaces:**

- `ScanKeysInput.key_type: Option<String>` / `ScanKeysInput.key_type: string | null`。
- `KeySummary` 增加 `memory_bytes: Option<u64>`、`encoding: Option<String>`、`idle_seconds: Option<u64>`。
- `ExportKeysInput { connection_id: String, keys: Vec<String> }`。
- `ExportedKey { key: String, ttl_ms: i64, value: RedisValue }`。
- `ImportKeysInput { connection_id: String, entries: Vec<ExportedKey> }`。
- Bridge 导出 `exportKeys(input): Promise<ExportedKey[]>` 和 `importKeys(input): Promise<number>`。

- [ ] **Step 1: Write the failing test**

在 Rust domain 测试增加：

```rust
fn valid_scan() -> ScanKeysInput {
    ScanKeysInput {
        connection_id: "local".into(),
        cursor: 0,
        pattern: "*".into(),
        count: 100,
        key_type: None,
    }
}

#[test]
fn scan_filter_rejects_unknown_key_type_and_accepts_supported_type() {
    assert!(ScanKeysInput {
        connection_id: "local".into(),
        cursor: 0,
        pattern: "*".into(),
        count: 100,
        key_type: Some("hash".into()),
    }
    .validate()
    .is_ok());

    assert_eq!(ScanKeysInput { key_type: Some("vector".into()), ..valid_scan() }
        .validate()
        .unwrap_err(), AppError::InvalidConnection);
}

#[test]
fn import_rejects_empty_entries_and_exported_key_rejects_empty_name() {
    assert_eq!(ImportKeysInput { connection_id: "local".into(), entries: vec![] }
        .validate().unwrap_err(), AppError::InvalidConnection);
    let empty_name = ExportedKey {
        key: "".into(),
        ttl_ms: -1,
        value: RedisValue::String { value: "value".into() },
    };
    assert_eq!(empty_name.validate().unwrap_err(), AppError::InvalidConnection);
}
```

在 `src/lib/tauri.test.ts` 增加 wrapper 测试，断言命令名分别为 `export_keys`、`import_keys`，参数形状为 `{ input }`。

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test domain scan_filter -q` and `npm test -- --run src/lib/tauri.test.ts`

Expected: Rust 因字段/类型缺失失败，前端因 wrapper 未导出失败。

- [ ] **Step 3: Write minimal implementation**

在 Rust 增加 DTO、`validate()` 和 `RedisValue` 复用；类型过滤只接受 `string/hash/list/set/zset/stream/json`，模块别名 `ReJSON-RL/ReJSON-RS/JSON` 在后端归一化为 `json`。前端类型与 Rust serde 字段保持 snake_case，并在 `tauri.ts` 使用现有 `call<T>`。

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test domain scan_filter -q` and `npm test -- --run src/lib/tauri.test.ts`

Expected: 新增测试和既有 domain/bridge 测试 PASS。

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/domain/key.rs src-tauri/src/domain/mod.rs src/lib/types.ts src/lib/tauri.ts src-tauri/tests/domain.rs src/lib/tauri.test.ts
git commit -m "扩展 Browser 搜索与导入导出数据契约"
```

### Task 3: 实现 Redis Browser 查询、元数据和导入导出 commands

**Files:**

- Modify: `src-tauri/src/redis/connection_manager.rs`
- Modify: `src-tauri/src/commands/browser.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/lib.rs`
- Test: `src-tauri/tests/commands.rs`、`src-tauri/tests/redis_integration.rs`

**Interfaces:**

- Extend `RedisOperations` with `export_keys(input: ExportKeysInput) -> Result<Vec<ExportedKey>, AppError>` and `import_keys(input: ImportKeysInput) -> Result<u64, AppError>`.
- Keep `scan_keys(input)` as the only list entry point; it returns the SCAN cursor even when type filtering removes items.
- Add Tauri adapters `export_keys` and `import_keys` in `commands/browser.rs`, then register them in `commands/mod.rs`, `src-tauri/src/lib.rs` and the command adapter test.

- [ ] **Step 1: Write the failing test**

在 `src-tauri/tests/commands.rs` 的 command adapter test 中先引用：

```rust
let _ = browser::export_keys;
let _ = browser::import_keys;
```

在 `redis_integration.rs` 增加真实 Redis 场景：写入一个 String 和 Hash，调用 `export_keys` 得到两个 DTO，再调用 `import_keys` 写入新键，断言返回 2、原键未被覆盖；扫描传入 `key_type: Some("hash")` 时只返回 Hash 摘要。

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test commands exposes_all_tauri_command_adapters -q`

Expected: FAIL，因为 trait 方法和 command adapter 尚不存在；集成测试不能编译也属于预期 RED。

- [ ] **Step 3: Write minimal implementation**

在 `scan_keys` 的现有 `TYPE`、`PTTL`、`key_size` 流程中：

- 先执行一次 `SCAN`，保持 Redis 返回的 cursor。
- 对扫描到的每个 key 生成 `KeySummary`；若 `key_type` 不匹配则跳过，不重新从 cursor 0 扫描。
- `memory_bytes`、`encoding`、`idle_seconds` 通过 `MEMORY USAGE`、`OBJECT ENCODING`、`OBJECT IDLETIME` best-effort 读取，单项失败映射为 `None`。

实现 `export_keys`：校验 connection/key 列表，逐个调用现有安全的 `read_key` 和 `PTTL`，返回 `ExportedKey`；未知类型返回 `UnsupportedDataType`。实现 `import_keys`：校验所有 DTO 和非空 key，逐个拒绝已存在键，复用 `write_key`，按 `ttl_ms` 调用 `PEXPIRE`，返回成功数；不使用 `execute_command` 解析导入文件内容。

所有新 command 只返回 `AppError` 或 typed DTO，不回传 Redis 错误文本。

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check` and `cargo test --manifest-path src-tauri/Cargo.toml --test commands --test domain -q`

Expected: command/domain tests PASS。

若本机 Redis 可用，再运行：

```bash
REDIX_TEST_REDIS_URL=redis://127.0.0.1:6379 cargo test --manifest-path src-tauri/Cargo.toml --test redis_integration -- --ignored --nocapture
```

Expected: Browser export/import/filter 流程 PASS；若环境或模块不可用，测试必须断言稳定错误，而不是吞掉失败。

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/redis/connection_manager.rs src-tauri/src/commands/browser.rs src-tauri/src/commands/mod.rs src-tauri/src/lib.rs src-tauri/tests/commands.rs src-tauri/tests/redis_integration.rs
git commit -m "实现 Browser 类型过滤与本地导入导出"
```

### Task 4: 实现 Browser 前端筛选、元数据和原生文件流

**Files:**

- Create: `src/features/browser/BrowserImportExport.tsx`
- Modify: `src/features/browser/browserState.ts`
- Modify: `src/features/browser/BrowserPage.tsx`
- Modify: `src/features/browser/KeyList.tsx`
- Modify: `src/features/browser/KeyDetails.tsx`
- Modify: `src/features/browser/browserState.test.ts`
- Modify: `src/features/browser/browser.test.tsx`

**Interfaces:**

- `BrowserPageState` 增加 `keyType: string`、`metadata: KeyInfo | null` 和 `selectedKeys: string[]` 的单一状态来源。
- `applyScanPage` 必须在 `replace=true` 时清空选择和详情，在 `replace=false` 时保留选择但去掉不在当前 key 列表中的项。
- `BrowserImportExport` 接受 `{ connectionId: string; selectedKeys: string[]; onImported(): Promise<void> }`。

- [ ] **Step 1: Write the failing test**

在 `browserState.test.ts` 增加纯函数测试：

```ts
it("按类型过滤扫描摘要并在刷新时清空选择", () => {
  const state = { ...initialBrowserPageState, selectedKeys: ["user:1"] };
  const next = applyScanPage(state, {
    cursor: 0,
    has_more: false,
    keys: [
      { key: "user:1", key_type: "string", ttl_ms: -1, size: 1 },
      { key: "user:2", key_type: "hash", ttl_ms: -1, size: 2 },
    ],
  }, true, "hash");
  expect(next.keys.map((key) => key.key)).toEqual(["user:2"]);
  expect(next.selectedKeys).toEqual([]);
});
```

在 `browser.test.tsx` 增加：类型选择触发 `{ key_type: "hash" }`、刷新重置 cursor、勾选键后导出只调用一次 `exportKeys`、导入文件解析后调用 `importKeys`、Blob 下载不泄露密码。

- [ ] **Step 2: Run test to verify it fails**

Run: `npm test -- --run src/features/browser/browserState.test.ts src/features/browser/browser.test.tsx`

Expected: FAIL，因为 state 没有类型/选择字段，bridge 和 ImportExport 组件不存在。

- [ ] **Step 3: Write minimal implementation**

在 `browserState.ts` 增加 `keyType`、`selectedKeys`、`metadata`，让 `applyScanPage` 接收可选类型并保持 cursor 语义。`BrowserPage` 将类型筛选传给 `scanKeys`，刷新时递增 request token；列表选择使用稳定 key，不在渲染循环中调用 IPC。

`BrowserImportExport` 使用隐藏 `<input type="file" accept="application/json">`：

```ts
const raw = await file.text();
const payload = JSON.parse(raw) as ImportKeysInput["entries"];
await importKeys({ connection_id: connectionId, entries: payload });
```

导出先调用 `exportKeys`，再 `JSON.stringify`、`Blob`、`URL.createObjectURL`、`<a download>` 下载并立即 `URL.revokeObjectURL`。解析错误、空选择、重复键和后端错误使用现有固定中文错误映射。元数据刷新复用 `getKeyInfo`，失败只显示可选字段不可用，不清空已读 value。

- [ ] **Step 4: Run test to verify it passes**

Run: `npm test -- --run src/features/browser/browserState.test.ts src/features/browser/browser.test.tsx` and `npm run build`

Expected: Browser focused tests and TypeScript/Vite build PASS。

- [ ] **Step 5: Commit**

```bash
git add src/features/browser/BrowserImportExport.tsx src/features/browser/browserState.ts src/features/browser/BrowserPage.tsx src/features/browser/KeyList.tsx src/features/browser/KeyDetails.tsx src/features/browser/browserState.test.ts src/features/browser/browser.test.tsx
git commit -m "完善 Browser 搜索过滤元数据和文件操作"
```

### Task 5: Browser 子计划验证和文档

**Files:**

- Modify: `README.md`
- Modify: `docs/non-cloud-scope.md`
- Modify: `progress.md`
- Modify: `task_plan.md`

- [ ] **Step 1: Write the failing scope/documentation test**

在现有非 Cloud scope 测试中增加 Browser 新增入口断言：生产入口可以包含 `export_keys`、`import_keys`、`key_type`，但不得出现 Cloud/Azure/AI/Telemetry 命中。

- [ ] **Step 2: Run test to verify it fails**

Run: `npm test -- --run scripts/check-non-cloud-scope.test.mjs`

Expected: 若扫描 fixture 尚未覆盖新入口，测试先以缺失断言失败；不要修改扫描器去忽略真实生产入口。

- [ ] **Step 3: Write minimal implementation**

更新 README 的 Browser 能力、导入导出格式和模块降级说明；更新 `docs/non-cloud-scope.md` 的排除项和验证命令；将本子计划实际完成项、测试结果和环境限制写入 `progress.md`。

- [ ] **Step 4: Run test to verify it passes**

Run: `npm run check:non-cloud && npm run test:frontend && npm run build && cargo fmt --manifest-path src-tauri/Cargo.toml -- --check && cargo test --manifest-path src-tauri/Cargo.toml && git diff --check`

Expected: 全部退出码为 0；未执行或被环境阻断的真实 Redis 测试必须在进度记录中如实说明。

- [ ] **Step 5: Commit**

```bash
git add README.md docs/non-cloud-scope.md progress.md task_plan.md
git commit -m "记录 Browser 生产力增强范围和验证结果"
```
