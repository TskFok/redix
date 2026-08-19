# Query Library 与设置实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 使用共享的版本化 JSON 文档仓储补齐 Query Library 和本地设置，支持查询的新增、编辑、删除、搜索、回填 Workbench，以及主题、结果格式、扫描数量和多命令错误策略偏好。

**Architecture:** Rust 负责本地资源的版本迁移、敏感命令校验、原子保存和固定错误映射；前端通过 typed Tauri wrapper 维护页面状态。Query Library 和 Settings 使用独立 JSON 文件，损坏文件恢复默认值但不覆盖原文件；Workbench、Browser 在启动时读取同一份设置并使用 Query Library 的回填回调。

**Tech Stack:** Rust、serde/serde_json、Tauri 2、React、TypeScript、Vitest、Testing Library、现有 `JsonDocumentStore`。

**Spec:** `docs/superpowers/specs/2026-08-18-redisinsight-non-cloud-parity-design.md`

## Global Constraints

- 必须复用 Browser 子计划建立的 `JsonDocumentStore`；不使用 `localStorage`、SQL、SQLite 或第二套原子写入逻辑。
- Workbench 子计划提供 `is_sensitive_command` 和命令回填接口；Query Library 与自动历史使用同一命令族规则。
- Query Library 只保存普通 Redis 命令；`AUTH`、`HELLO`、`ACL`、`CONFIG` 命令族拒绝保存。
- JSON 文件不保存密码、URI、Redis 原始错误文本；损坏或版本未知时保留原文件并返回安全默认值/固定错误码。
- 设置值必须在 Rust 和前端两端校验：主题只能是 system/light/dark，结果格式只能是 raw/text/json，扫描数量范围为 10 到 1000。
- 当前 `main` 分支直接修改，不创建分支；提交信息使用简体中文；每个任务先 RED 再 GREEN，结束运行 `git diff --check`。

## 文件地图

- Create: `src-tauri/src/domain/local_resources.rs` — Query Library/Settings DTO、版本迁移和校验。
- Modify: `src-tauri/src/domain/mod.rs` — 导出本地资源模型。
- Modify: `src-tauri/src/lib.rs` — 使用 Workbench 子计划已加入的 `AppState.data_dir`。
- Create: `src-tauri/src/commands/query_library.rs`、`src-tauri/src/commands/settings.rs` — 本地资源 commands。
- Modify: `src-tauri/src/commands/mod.rs`、`src-tauri/src/lib.rs` — 导出和注册 commands。
- Test: `src-tauri/tests/domain.rs`、`src-tauri/tests/persistence.rs`、`src-tauri/tests/commands.rs` — 迁移、损坏恢复、CRUD 和 command adapter。
- Modify: `src/lib/types.ts`、`src/lib/tauri.ts`、`src/lib/tauri.test.ts` — typed DTO 和 wrapper。
- Create: `src/features/query-library/queryLibraryState.ts`、`src/features/query-library/QueryLibraryPage.tsx`、`src/features/query-library/queryLibrary.test.tsx`。
- Create: `src/features/settings/settingsState.ts`、`src/features/settings/SettingsPage.tsx`、`src/features/settings/settings.test.tsx`。
- Modify: `src/App.tsx`、`src/styles.css`、`src/features/workbench/WorkbenchPage.tsx`、`src/features/browser/BrowserPage.tsx`、`src/app.smoke.test.tsx`。

### Task 1: 定义本地资源 DTO、版本迁移和安全校验

**Files:**

- Create: `src-tauri/src/domain/local_resources.rs`
- Modify: `src-tauri/src/domain/mod.rs`
- Test: `src-tauri/tests/domain.rs`
- Test: `src-tauri/tests/persistence.rs`

**Interfaces:**

~~~rust
pub struct QueryLibraryItem {
    pub id: String,
    pub name: String,
    pub command: String,
    pub tags: Vec<String>,
    pub updated_at: u64,
}

pub struct QueryLibraryItemInput {
    pub id: Option<String>,
    pub name: String,
    pub command: String,
    pub tags: Vec<String>,
}

pub struct QueryLibraryDocument {
    pub version: u32,
    pub items: Vec<QueryLibraryItem>,
}

pub struct AppSettings {
    pub version: u32,
    pub theme: String,
    pub result_format: String,
    pub scan_count: u32,
    pub continue_on_error: bool,
}
~~~

实现 `Default`：Query Library 版本为 1 且 items 为空；Settings 默认 `system/raw/100/false`。实现 `VersionedJsonDocument` 的 `version` 和 `migrate`；版本 0 或缺失 version 的旧文档补齐默认字段并迁移到版本 1，未知版本返回 `AppError::PersistenceFailed`。实现 `QueryLibraryItemInput::validate`、`AppSettings::validate` 和 `normalize_query_library_item`：名称非空且不超过 120 字符，命令非空且不超过 10,000 字符，id 只允许本地生成的安全字符，标签最多 20 个且单个不超过 40 字符；使用 Workbench 的 `is_sensitive_command` 拒绝敏感命令。

- [ ] **Step 1: Write the failing test**

在 `src-tauri/tests/domain.rs` 增加：

~~~rust
#[test]
fn query_library_rejects_sensitive_commands_and_accepts_normal_commands() {
    assert!(QueryLibraryItemInput {
        id: None,
        name: "读取用户".into(),
        command: "GET user:1".into(),
        tags: vec!["用户".into()],
    }
    .validate()
    .is_ok());
    assert_eq!(
        QueryLibraryItemInput {
            id: None,
            name: "认证".into(),
            command: "AUTH secret".into(),
            tags: vec![],
        }
        .validate()
        .unwrap_err(),
        AppError::InvalidConnection
    );
}

#[test]
fn settings_validate_fixed_enum_and_range() {
    assert!(AppSettings {
        version: 1,
        theme: "dark".into(),
        result_format: "json".into(),
        scan_count: 200,
        continue_on_error: true,
    }
    .validate()
    .is_ok());
    assert_eq!(
        AppSettings { scan_count: 1, ..AppSettings::default() }
            .validate()
            .unwrap_err(),
        AppError::InvalidConnection
    );
}
~~~

在 `src-tauri/tests/persistence.rs` 增加版本 0 Query Library 和 Settings 文档迁移测试，并断言迁移后版本为 1；增加损坏 JSON 读取后返回默认文档、原文件内容保持不变的测试。

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test domain query_library_rejects -q`

Expected: FAIL，因为本地资源 DTO、校验和迁移实现尚不存在。

- [ ] **Step 3: Write minimal implementation**

在 `local_resources.rs` 使用 serde_json::Value 做显式版本迁移，不用 `unwrap` 处理用户文件。迁移只在内存中构造新文档，`JsonDocumentStore::load_or_default` 在 JSON 解析/迁移失败时返回默认值但不写回源文件；文件权限、目录创建和原子替换失败仍返回 `PersistenceFailed`。id 在保存时由 Rust 生成，updated_at 使用当前 Unix 毫秒但测试只验证非零和排序，不比较固定时间。

- [ ] **Step 4: Run test to verify it passes**

Run:

~~~bash
cargo test --manifest-path src-tauri/Cargo.toml --test domain query_library_rejects -q
cargo test --manifest-path src-tauri/Cargo.toml --test persistence -q
~~~

Expected: 本地资源校验、版本迁移、损坏文件保护和既有 persistence 测试 PASS。

- [ ] **Step 5: Commit**

~~~bash
git add src-tauri/src/domain/local_resources.rs src-tauri/src/domain/mod.rs src-tauri/tests/domain.rs src-tauri/tests/persistence.rs
git commit -m "增加 Query Library 与设置本地资源模型"
~~~

### Task 2: 实现 Query Library/Settings Rust commands

**Files:**

- Create: `src-tauri/src/commands/query_library.rs`
- Create: `src-tauri/src/commands/settings.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/lib.rs`
- Test: `src-tauri/tests/commands.rs`
- Test: `src-tauri/tests/persistence.rs`

**Interfaces:**

- `list_query_library() -> Result<Vec<QueryLibraryItem>, AppError>`
- `save_query_library_item(input: QueryLibraryItemInput) -> Result<QueryLibraryItem, AppError>`
- `delete_query_library_item(id: String) -> Result<(), AppError>`
- `get_app_settings() -> Result<AppSettings, AppError>`
- `save_app_settings(settings: AppSettings) -> Result<AppSettings, AppError>`

路径固定为 `AppState.data_dir.join("query-library.json")` 和 `AppState.data_dir.join("settings.json")`。每个 command 通过 `JsonDocumentStore` load/save；Query Library 最多保存 500 项，按 `updated_at` 降序返回；保存已存在 id 时更新，不存在时生成新 id；删除不存在 id 返回 `InvalidConnection`。Settings 保存前执行完整校验，返回保存后的规范 DTO。损坏文件读取返回默认值但不覆盖原文件，真正的文件系统错误返回 `PersistenceFailed`。

- [ ] **Step 1: Write the failing test**

在 `src-tauri/tests/commands.rs` 的 command adapter 测试先引用：

~~~rust
let _ = query_library::list_query_library;
let _ = query_library::save_query_library_item;
let _ = query_library::delete_query_library_item;
let _ = settings::get_app_settings;
let _ = settings::save_app_settings;
~~~

在 persistence/command 测试增加：保存普通查询后可 list、更新同一 id 不产生重复项、删除后 list 为空；保存 `CONFIG SET requirepass secret` 返回固定 `InvalidConnection`；settings 保存后重新读取保持字段；损坏文件读取默认值且原文件字节不变。

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test commands exposes_all_tauri_command_adapters -q`

Expected: FAIL，因为 commands 模块和 Tauri 注册尚不存在。

- [ ] **Step 3: Write minimal implementation**

为测试提供 `list_query_library_inner`、`save_query_library_item_inner`、`delete_query_library_item_inner`、`get_app_settings_inner` 和 `save_app_settings_inner`，参数接收 `&AppState`，避免测试依赖真实 Tauri runtime。读-改-写过程只在单次 command 内执行，不把任何 Redis 查询放进循环；Query Library 的排序使用内存向量排序后一次写入。

- [ ] **Step 4: Run test to verify it passes**

Run:

~~~bash
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo test --manifest-path src-tauri/Cargo.toml --test commands --test domain --test persistence -q
~~~

Expected: Rust commands、迁移、敏感命令过滤和原子文件回归 PASS。

- [ ] **Step 5: Commit**

~~~bash
git add src-tauri/src/commands/query_library.rs src-tauri/src/commands/settings.rs src-tauri/src/commands/mod.rs src-tauri/src/lib.rs src-tauri/tests/commands.rs src-tauri/tests/persistence.rs
git commit -m "实现 Query Library 和设置 commands"
~~~

### Task 3: 接入 typed bridge 和 Query Library 页面

**Files:**

- Modify: `src/lib/types.ts`
- Modify: `src/lib/tauri.ts`
- Modify: `src/lib/tauri.test.ts`
- Create: `src/features/query-library/queryLibraryState.ts`
- Create: `src/features/query-library/QueryLibraryPage.tsx`
- Create: `src/features/query-library/queryLibrary.test.tsx`

**Interfaces:**

- `listQueryLibrary(): Promise<QueryLibraryItem[]>`
- `saveQueryLibraryItem(input: QueryLibraryItemInput): Promise<QueryLibraryItem>`
- `deleteQueryLibraryItem(id: string): Promise<void>`
- `QueryLibraryPageProps { onFill(command: string): void }`
- 页面状态包含 `items`、`query`、`loading`、`saving`、`editing`、`error` 和 `requestId`。

- [ ] **Step 1: Write the failing test**

在 `src/lib/tauri.test.ts` 断言三个 wrapper 的 invoke 命令名和 payload 形状。在 `queryLibrary.test.tsx` 增加：

- 首次加载渲染查询名称、命令和标签。
- 搜索按名称、命令和标签过滤，空结果显示稳定空状态。
- 新增/编辑保存只调用一次 `saveQueryLibraryItem`；删除先确认再调用一次 delete。
- 点击“回填 Workbench”只调用 `onFill(command)`，不直接执行 Redis command。
- 后端拒绝敏感命令时显示固定错误，不在页面显示原始错误文本。
- 组件卸载或连续刷新时忽略旧响应。

- [ ] **Step 2: Run test to verify it fails**

Run: `npm test -- --run src/lib/tauri.test.ts src/features/query-library/queryLibrary.test.tsx`

Expected: FAIL，因为类型、bridge 和 Query Library 页面不存在。

- [ ] **Step 3: Write minimal implementation**

页面初次挂载调用 `listQueryLibrary`；搜索在内存中过滤，不为每个字符调用 IPC。编辑表单只提交 typed DTO，命令只做展示和回填。保存成功后用返回项替换同 id 或追加新项；删除成功后从 state 移除；请求 token 防止旧响应覆盖新列表。显示名称、命令、标签、更新时间和操作按钮，空状态提供“新建查询”入口。

- [ ] **Step 4: Run test to verify it passes**

Run: `npm test -- --run src/lib/tauri.test.ts src/features/query-library/queryLibrary.test.tsx && npm run build`

Expected: Query Library focused tests、bridge tests 和生产构建 PASS。

- [ ] **Step 5: Commit**

~~~bash
git add src/lib/types.ts src/lib/tauri.ts src/lib/tauri.test.ts src/features/query-library/queryLibraryState.ts src/features/query-library/QueryLibraryPage.tsx src/features/query-library/queryLibrary.test.tsx
git commit -m "增加 Query Library 页面和 typed bridge"
~~~

### Task 4: 接入设置页面并让 Browser/Workbench 使用偏好

**Files:**

- Create: `src/features/settings/settingsState.ts`
- Create: `src/features/settings/SettingsPage.tsx`
- Create: `src/features/settings/settings.test.tsx`
- Modify: `src/lib/types.ts`
- Modify: `src/lib/tauri.ts`
- Modify: `src/lib/tauri.test.ts`
- Modify: `src/features/browser/BrowserPage.tsx`
- Modify: `src/features/workbench/WorkbenchPage.tsx`

**Interfaces:**

- `getAppSettings(): Promise<AppSettings>`
- `saveAppSettings(settings: AppSettings): Promise<AppSettings>`
- `SettingsPageProps { settings: AppSettings; onSaved(settings: AppSettings): void }`
- `settingsState.ts` 提供默认值、前端枚举/范围校验、固定错误映射。
- `BrowserPage` 接受 `scanCount: number`；`WorkbenchPage` 接受 `defaultFormat` 和 `defaultContinueOnError`，用户操作后只更新当前页面状态。

- [ ] **Step 1: Write the failing test**

在 `settings.test.tsx` 增加：

- 加载默认设置后渲染主题、结果格式、扫描数量和错误策略控件。
- 非法扫描数量在提交前被阻止；合法保存调用一次 `saveAppSettings`。
- 保存成功通过 `onSaved` 回传规范设置，失败显示固定错误。
- 主题选择更新根元素的 `data-theme`，重新挂载后从保存设置恢复。
- Browser 首次扫描使用 settings 的 `scan_count`；Workbench 初始结果格式和继续执行选项使用 settings 的对应值。

在 `src/lib/tauri.test.ts` 增加 `get_app_settings`/ `save_app_settings` wrapper 测试。

- [ ] **Step 2: Run test to verify it fails**

Run: `npm test -- --run src/lib/tauri.test.ts src/features/settings/settings.test.tsx`

Expected: FAIL，因为设置 bridge、页面和 Browser/Workbench props 尚不存在。

- [ ] **Step 3: Write minimal implementation**

设置页面使用受控表单；保存前 trim/parse 数值并校验 10 到 1000，显示固定中文错误。App 在根节点维护 `data-theme`，system 使用系统媒体查询，light/dark 使用显式主题；设置加载失败使用默认值并提示一次。Browser 将 `scanCount` 传给首轮和后续 `scanKeys`，Workbench 只在加载时应用默认值，不覆盖用户已经修改的当前状态。

- [ ] **Step 4: Run test to verify it passes**

Run:

~~~bash
npm test -- --run src/lib/tauri.test.ts src/features/settings/settings.test.tsx src/features/browser/browser.test.tsx src/features/workbench/workbench.test.tsx
npm run build
git diff --check
~~~

Expected: 设置、Browser、Workbench 相关测试和生产构建 PASS，既有用户选择不会被异步设置覆盖。

- [ ] **Step 5: Commit**

~~~bash
git add src/features/settings/settingsState.ts src/features/settings/SettingsPage.tsx src/features/settings/settings.test.tsx src/lib/types.ts src/lib/tauri.ts src/lib/tauri.test.ts src/features/browser/BrowserPage.tsx src/features/workbench/WorkbenchPage.tsx
git commit -m "增加设置页面并应用本地偏好"
~~~

### Task 5: 接入应用导航和 Query Library 回填 Workbench

**Files:**

- Modify: `src/App.tsx`
- Modify: `src/lib/types.ts`
- Modify: `src/styles.css`
- Modify: `src/features/workbench/WorkbenchPage.tsx`
- Modify: `src/app.smoke.test.tsx`

**Interfaces:**

- 将 `Workspace` 扩展为 `"browser" | "workbench" | "database" | "query-library" | "settings"`。
- `App` 维护 `pendingWorkbenchCommand: string | null`；Query Library 的 `onFill` 设置命令并切换到 Workbench。
- `WorkbenchPage` 接受 `initialCommand?: string` 和 `onCommandConsumed?: () => void`；收到新命令后只回填输入，不自动执行，消费后清空 pending 值。
- Settings 页面由 App 维护已加载设置，并通过 `onSaved` 更新 Browser/Workbench 的默认 props 和主题。

- [ ] **Step 1: Write the failing test**

在 `src/app.smoke.test.tsx` 增加：

- 连接后 Query Library 和 Settings 导航可用；未连接时 Redis 工作区仍禁用。
- 点击 Query Library 的“回填 Workbench”后 active section 为 Workbench，命令输入框出现保存的命令，Redis 未被自动执行。
- 点击 Settings 保存主题后应用根节点主题属性改变，刷新页面仍从 settings command 恢复。
- Cloud/Azure/AI/Telemetry/远程插件入口不存在。

- [ ] **Step 2: Run test to verify it fails**

Run: `npm test -- --run src/app.smoke.test.tsx`

Expected: FAIL，因为新导航、pending command 和设置注入尚不存在。

- [ ] **Step 3: Write minimal implementation**

新增 Query Library、Settings 导航图标和描述；在 App 中集中加载 settings，并用显式回调更新 active settings。Query Library 只传递命令字符串；Workbench 用 effect 监听 `initialCommand`，设置 textarea 值后调用 `onCommandConsumed`，不触发 execute。保持无 active profile 时 Query Library/Settings 可访问但 Browser/Workbench/Database 禁用；Query Library 的保存数据不依赖 Redis 连接。

- [ ] **Step 4: Run test to verify it passes**

Run:

~~~bash
npm test -- --run src/app.smoke.test.tsx src/features/query-library/queryLibrary.test.tsx src/features/settings/settings.test.tsx
npm run test:frontend
npm run build
git diff --check
~~~

Expected: 五个工作区导航、回填、主题和未连接状态回归 PASS。

- [ ] **Step 5: Commit**

~~~bash
git add src/App.tsx src/lib/types.ts src/styles.css src/features/workbench/WorkbenchPage.tsx src/app.smoke.test.tsx
git commit -m "接入 Query Library 设置和 Workbench 回填"
~~~

### Task 6: Query Library/设置子计划范围审查和文档记录

**Files:**

- Modify: `README.md`
- Modify: `docs/non-cloud-scope.md`
- Modify: `progress.md`
- Modify: `task_plan.md`

- [ ] **Step 1: Write the failing regression test**

在应用 smoke 测试增加本地资源边界回归：Query Library/Settings 只读写应用数据目录，不显示 Cloud/Azure/AI/Telemetry/远程插件内容；损坏 JSON 恢复默认值且原文件内容保持不变。

- [ ] **Step 2: Run test to verify it fails**

Run: `npm test -- --run src/app.smoke.test.tsx src/features/settings/settings.test.tsx`

Expected: FAIL，因为本地资源入口和损坏文件边界尚未实现。

- [ ] **Step 3: Write minimal implementation**

更新 README 和 `docs/non-cloud-scope.md`，明确 Query Library/Settings 的文件名、版本迁移、敏感命令规则和默认值；在 `progress.md` 记录真实验证结果与环境限制，在 `task_plan.md` 勾选 Phase 8 第一批完成项。

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

Expected: 非 Cloud 扫描、前端、构建、Rust 和差异检查全部 PASS；无 localStorage、SQL 或敏感信息落盘。

- [ ] **Step 5: Commit**

~~~bash
git add README.md docs/non-cloud-scope.md progress.md task_plan.md
git commit -m "记录 Query Library 与设置范围和验证结果"
~~~
