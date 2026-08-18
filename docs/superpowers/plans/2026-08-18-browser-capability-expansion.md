# Browser 能力扩展 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox syntax (- [ ]) for tracking.

**Goal:** 在本地 Redis Standalone 上补齐 Browser 的新增、重命名、批量删除、元数据刷新，并支持基础 Stream 与 RedisJSON 根文档编辑。

**Architecture:** 扩展现有 Rust RedisOperations 和 Tauri typed commands，所有 Redis 访问继续集中在 RedisService；React 仅消费 DTO，通过 BrowserPage 管理请求生命周期。新增值类型沿用 RedisValue 枚举，批量删除使用单个 DEL 命令，长连接运维能力留到后续计划。

**Tech Stack:** Rust 2021、Tauri 2、redis crate 1.5、serde/serde_json、React 19、TypeScript、Vite、Vitest、Testing Library。

**Spec:** docs/superpowers/specs/2026-08-18-browser-capability-expansion-design.md

## Global Constraints

- 默认在当前 main 分支工作，不创建新分支或 worktree。
- 保留用户已有未提交变更；只修改本计划列出的文件和新增文件。
- 生产代码不得包含 Redis Cloud、Azure 云登录/发现、AI、Telemetry 或远程插件入口。
- Browser 扫描只能使用 SCAN cursor MATCH pattern COUNT count，不得使用 KEYS。
- 禁止在循环遍历中查询 SQL；本批次不引入 SQL。
- Redis 底层错误统一映射为固定 AppError，不得向 UI 回传 URI、密码或原始 Redis 错误文本。
- 每个行为变更严格执行 RED → GREEN → REFACTOR；先看到失败测试再写生产实现。
- 每个 commit message 使用简体中文；不提交用户已有未提交文件。

---

### Task 1：扩展领域 DTO 与纯函数边界

**Files:**
- Modify: src-tauri/src/domain/key.rs
- Modify: src-tauri/src/domain/mod.rs
- Modify: src-tauri/src/error.rs
- Modify: src-tauri/src/redis/connection_manager.rs
- Modify: src-tauri/src/redis/key_ops.rs
- Test: src-tauri/tests/domain.rs
- Test: src-tauri/src/redis/key_ops.rs

**Interfaces:**
- Produces CreateKeyInput with connection_id, key, value: RedisValue, ttl_ms: Option<i64>。
- Produces RenameKeyInput with connection_id, key, new_key。
- Produces DeleteKeysInput with connection_id, keys: Vec<String>。
- Produces KeyInfoInput with connection_id, key and KeyInfo with key, key_type, ttl_ms, size, memory_bytes, encoding, idle_seconds。
- Extends RedisValue with Json { value: serde_json::Value } and Stream { entries: Vec<StreamEntry> }；StreamEntry contains id and fields: Vec<StreamField>。

- [x] Step 1: Write failing Rust tests for DTO validation and codecs

  Add tests that assert empty key/new key, duplicate Stream field names, empty Stream entries, negative TTL other than None, invalid JSON text and malformed Stream wire fields return AppError::InvalidConnection or AppError::CommandFailed without carrying input text. Add valid JSON and Stream round-trip fixtures.

- [x] Step 2: Run the focused Rust tests and verify RED

  ~~~bash
  CARGO_NET_OFFLINE=true cargo test --manifest-path src-tauri/Cargo.toml --lib
  CARGO_NET_OFFLINE=true cargo test --manifest-path src-tauri/Cargo.toml --test domain
  ~~~

  Expected: FAIL because the new DTOs, enum variants and pure helpers do not yet exist.

- [x] Step 3: Implement the smallest domain and codec surface

  Add the DTOs and validation methods, the two RedisValue variants, and pure helpers that turn Stream entry field/value pairs into StreamEntry while rejecting odd-length field lists. Keep JSON parsing in serde_json; do not add a new crate.

- [x] Step 4: Re-run focused tests and refactor only after GREEN

  Run the same command and require all focused tests to pass. Then run cargo fmt --manifest-path src-tauri/Cargo.toml -- --check and keep the public DTO field names in snake_case for Tauri serialization.

- [x] Step 5: Commit only Task 1 files

  ~~~bash
  git add src-tauri/src/domain/key.rs src-tauri/src/domain/mod.rs src-tauri/src/error.rs src-tauri/src/redis/connection_manager.rs src-tauri/src/redis/key_ops.rs src-tauri/tests/domain.rs
  git commit -m "扩展 Browser 数据类型领域模型"
  ~~~

### Task 2：实现 Redis Browser 扩展操作

**Files:**
- Modify: src-tauri/src/redis/connection_manager.rs
- Modify: src-tauri/src/redis/mod.rs
- Modify: src-tauri/src/commands/browser.rs
- Modify: src-tauri/src/commands/mod.rs
- Modify: src-tauri/src/lib.rs
- Test: src-tauri/tests/redis_integration.rs
- Test: src-tauri/tests/commands.rs

**Interfaces:**
- Consumes Task 1 DTOs and RedisValue Json/Stream。
- Extends RedisOperations with create_key, rename_key, delete_keys, get_key_info。
- Tauri commands expose create_key, rename_key, delete_keys, get_key_info using { input } payloads and return KeyValue, KeyValue, u64, KeyInfo respectively。

- [x] Step 1: Add failing command and integration expectations

  Add command adapter references in src-tauri/tests/commands.rs and extend the ignored Redis flow with unique JSON/Stream keys, create/rename/batch-delete and get_key_info assertions. Keep JSON assertions conditional on JSON.SET availability and require UNSUPPORTED_DATA_TYPE when RedisJSON is absent.

- [x] Step 2: Run command tests and compile the ignored integration test to verify RED

  ~~~bash
  CARGO_NET_OFFLINE=true cargo test --manifest-path src-tauri/Cargo.toml --test commands --test redis_integration
  ~~~

  Expected: FAIL to compile because the new operation methods and command adapters are missing.

- [x] Step 3: Implement safe Redis operations

  In RedisService, validate all new inputs, check EXISTS before create, use one RENAMENX command for rename, pass all keys to one DEL for batch delete, and make optional MEMORY USAGE/OBJECT metadata failures return None. Extend TYPE dispatch for stream, ReJSON, and ReJSON-RL; read Stream with XRANGE - + COUNT 500, JSON with JSON.GET key ., and write JSON with JSON.SET key . serialized-value. Preserve fixed error mapping and ensure unknown module types do not abort a SCAN page.

- [x] Step 4: Run focused tests, format, and refactor after GREEN

  Run:

  ~~~bash
  CARGO_NET_OFFLINE=true cargo test --manifest-path src-tauri/Cargo.toml --test commands --test redis_integration
  cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
  ~~~

  Expected: command tests pass; the Redis integration test remains explicitly ignored unless REDIX_TEST_REDIS_URL is set. Fix any Redis crate type conversion issues in the service, not in tests.

- [x] Step 5: Commit only Task 2 files

  ~~~bash
  git add src-tauri/src/redis/connection_manager.rs src-tauri/src/redis/mod.rs src-tauri/src/commands/browser.rs src-tauri/src/commands/mod.rs src-tauri/src/lib.rs src-tauri/tests/redis_integration.rs src-tauri/tests/commands.rs
  git commit -m "实现 Browser 新增重命名批量删除和模块数据操作"
  ~~~

### Task 3：扩展前端类型与 Tauri bridge

**Files:**
- Modify: src/lib/types.ts
- Modify: src/lib/tauri.ts
- Modify: src/lib/tauri.test.ts
- Modify: src/features/connections/connectionState.ts
- Modify: src/features/browser/browserState.ts
- Modify: src/features/browser/KeyEditor.tsx
- Create: src/features/browser/browserState.test.ts

**Interfaces:**
- Produces TypeScript counterparts of Task 1 DTOs and RedisValue variants。
- Produces createKey, renameKey, deleteKeys, getKeyInfo wrappers with stable command names and { input } payloads。
- Produces redisValueKind values json and stream and fixed user-facing error mappings。

- [x] Step 1: Write failing bridge/state tests

  Add tests asserting each wrapper calls invoke with the exact snake_case command and nested input, plus state tests asserting JSON is parsed/serialized without mutation and Stream kind/label/clone behavior works.

- [x] Step 2: Run the focused frontend tests and verify RED

  ~~~bash
  npm test -- --run src/lib/tauri.test.ts src/features/browser/browserState.test.ts
  ~~~

  Expected: FAIL because wrappers, DTO variants and state helpers are missing.

- [x] Step 3: Implement typed wrappers and state helpers

  Extend the discriminated union, add typed bridge functions, normalize UNSUPPORTED_DATA_TYPE to a stable Chinese message, and make cloning deep-copy JSON values and Stream entries. Do not expose an arbitrary IPC error message.

- [x] Step 4: Run focused tests and build

  ~~~bash
  npm test -- --run src/lib/tauri.test.ts src/features/browser/browserState.test.ts
  npm run build
  ~~~

  Expected: focused tests and TypeScript build pass.

- [x] Step 5: Commit only Task 3 files

  ~~~bash
  git add src/lib/types.ts src/lib/tauri.ts src/lib/tauri.test.ts src/features/connections/connectionState.ts src/features/browser/browserState.ts src/features/browser/browserState.test.ts src/features/browser/KeyEditor.tsx
  git commit -m "补充 Browser 扩展 IPC 类型和状态"
  ~~~

### Task 4：实现新增键、选择和批量操作界面

**Files:**
- Create: src/features/browser/AddKey.tsx
- Create: src/features/browser/BulkKeyActions.tsx
- Modify: src/features/browser/BrowserPage.tsx
- Modify: src/features/browser/KeyList.tsx
- Modify: src/features/browser/browser.test.tsx
- Modify: src/styles.css

**Interfaces:**
- AddKey receives connectionId, busy, onCreated, onCancel and calls createKey。
- BulkKeyActions receives selected key names and calls deleteKeys once per confirmed action。
- BrowserPage owns selectedKeys, add-panel visibility, refresh request token and post-operation rescan。

- [x] Step 1: Write failing UI tests

  Add tests for opening 新增键, rejecting blank/duplicate key input from the component contract, creating a String and a Stream fixture, selecting two list rows, confirming one batch delete payload, clearing selection after refresh, and preventing stale create/delete responses after connection switch.

- [x] Step 2: Run Browser tests and verify RED

  ~~~bash
  npm test -- --run src/features/browser/browser.test.tsx
  ~~~

  Expected: FAIL because the add panel, row selection and bulk action controls do not exist.

- [x] Step 3: Implement minimal add/bulk flow

  Add accessible controls with aria-label/role=alert, use the existing window.confirm convention for destructive batch delete, call deleteKeys with one { connection_id, keys } payload, and re-run the current SCAN after create/rename/delete/refresh. Keep selection disabled while a request is active.

- [x] Step 4: Run Browser tests and visual build

  ~~~bash
  npm test -- --run src/features/browser/browser.test.tsx
  npm run build
  ~~~

  Expected: all Browser tests pass and the build emits no TypeScript errors.

- [x] Step 5: Commit only Task 4 files

  ~~~bash
  git add src/features/browser/AddKey.tsx src/features/browser/BulkKeyActions.tsx src/features/browser/BrowserPage.tsx src/features/browser/KeyList.tsx src/features/browser/browser.test.tsx src/styles.css
  git commit -m "实现 Browser 新增键和批量操作界面"
  ~~~

### Task 5：实现重命名、元数据和 Stream/JSON 编辑器

**Files:**
- Modify: src/features/browser/KeyDetails.tsx
- Modify: src/features/browser/KeyEditor.tsx
- Modify: src/features/browser/BrowserPage.tsx
- Modify: src/features/browser/browserState.ts
- Modify: src/features/browser/browser.test.tsx
- Modify: src/styles.css

**Interfaces:**
- KeyDetails calls renameKey and getKeyInfo, reports renamed detail with the new key name, and ignores stale responses。
- KeyEditor accepts all RedisValue variants and emits validated Stream/JSON values through the existing onSave callback。
- BrowserPage updates selected key identity and summary metadata through onDetailChange/rename callbacks。

- [x] Step 1: Write failing editor/detail tests

  Add tests for JSON pretty text parsing and invalid JSON rejection, Stream entry field editing and odd field rejection, rename payload/update, metadata refresh payload/display, unsupported-module error text, and preservation of existing five editor behaviors.

- [x] Step 2: Run focused Browser tests and verify RED

  ~~~bash
  npm test -- --run src/features/browser/browser.test.tsx
  ~~~

  Expected: FAIL because JSON/Stream controls, rename and metadata controls are missing.

- [x] Step 3: Implement editors and detail operations

  Extend editorTitle, clone and validation paths; use a formatted JSON textarea with JSON.parse, an entry/field table for Streams capped at 500 entries, a rename form that rejects blank names, and a metadata refresh button that only displays optional fields when returned. Preserve operation tokens across connection/key changes.

- [x] Step 4: Run focused/full tests and build

  ~~~bash
  npm test -- --run src/features/browser/browser.test.tsx
  npm run test:frontend
  npm run build
  ~~~

  Expected: Browser and full frontend tests pass, with no unhandled act warnings or accessibility query failures.

- [x] Step 5: Commit only Task 5 files

  ~~~bash
  git add src/features/browser/KeyDetails.tsx src/features/browser/KeyEditor.tsx src/features/browser/BrowserPage.tsx src/features/browser/browserState.ts src/features/browser/browser.test.tsx src/styles.css
  git commit -m "支持 Browser 重命名元数据和 Stream JSON 编辑"
  ~~~

### Task 6：真实 Redis 验证、范围审查与交付

**Files:**
- Modify: src-tauri/tests/redis_integration.rs
- Modify: README.md
- Modify: docs/non-cloud-scope.md
- Modify: findings.md
- Modify: progress.md
- Modify: task_plan.md

**Interfaces:**
- Verifies the completed Task 1–5 interfaces against Redis 8 Standalone and optional RedisJSON.
- Documents the supported Stream/JSON limits and clearly separates later RedisInsight batches。

- [x] Step 1: Run all offline and static checks

  ~~~bash
  npm run check:non-cloud
  npm run test:frontend
  npm run build
  cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
  CARGO_NET_OFFLINE=true cargo test --manifest-path src-tauri/Cargo.toml
  git diff --check
  ~~~

  Expected: all commands exit 0; the existing Redis integration test may remain ignored when no environment variable is present.

- [x] Step 2: Run the real Standalone integration flow when available

  ~~~bash
  REDIX_TEST_REDIS_URL=redis://127.0.0.1:6379 cargo test --manifest-path src-tauri/Cargo.toml --test redis_integration -- --ignored --nocapture
  ~~~

  Expected: five existing types plus Stream and create/rename/batch-delete/metadata pass; JSON either passes on a RedisJSON server or records stable UNSUPPORTED_DATA_TYPE on a vanilla Redis server.

- [x] Step 3: Perform a final non-Cloud and user-change diff review

  Inspect git diff --stat, git diff --check, the non-cloud scanner output, and git status --short; ensure existing user changes in src-tauri/Cargo.toml, src-tauri/src/commands/connections.rs, src-tauri/tests/commands.rs, src/App.tsx, src/app.smoke.test.tsx, src/styles.css, task_plan.md, findings.md, and progress.md are not overwritten or accidentally staged outside the intended additions.

- [x] Step 4: Update documentation and mark this phase complete

  Update README and non-cloud scope docs with the new Browser capabilities and explicit Stream/JSON limits. Append test output, errors and modified files to progress.md/findings.md, then mark the Browser expansion phase complete in task_plan.md.

- [x] Step 5: Commit only documentation updates not already committed

  ~~~bash
  git add README.md docs/non-cloud-scope.md
  git commit -m "完成 Browser 扩展范围说明"
  ~~~

## Self-Review Checklist

- [x] Scope matches the user-approved first batch: Browser productivity plus Stream/JSON, not later operations or topology work.
- [x] Every production interface is named in the spec and consumed by a later task.
- [x] Every task starts with a concrete failing test and a concrete command.
- [x] No task asks for Cloud, Azure, SQL, or unbounded module work.
- [x] The plan preserves the current Standalone connection model and existing five-value editor behavior.
- [x] Stream limits, RedisJSON optional dependency, stale async response protection, and error redaction are explicit.
