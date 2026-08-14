# Redix Tauri MVP Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (recommended) or `superpowers:executing-plans` to implement this plan task-by-task. Steps use checkbox syntax for tracking.

**Goal:** 创建一个基于 Tauri 2、React/Vite 和 Rust Redis domain 的桌面 Redis 客户端，交付 Standalone 连接、Browser、常用数据结构 CRUD 和 Workbench MVP。

**Architecture:** Tauri 2 作为桌面壳和 IPC 边界，Rust 负责连接、Redis 命令、数据结构转换、错误映射和本地持久化，React/Vite 负责连接页、Browser 和 Workbench。前端不直接连接 Redis；不启动本地 HTTP 服务，也不引入 SQL 数据库。

**Tech Stack:** Rust 2021、Tauri 2、`redis` Rust crate 1.5.x、`serde`/`serde_json`、`thiserror`、Tokio、`keyring` 4.1.x、React 19、Vite 7、TypeScript 5.8、Vitest 4、Testing Library。

## Global Constraints

- 首个 MVP 只支持 Redis Standalone TCP；Cluster、Sentinel、TLS、SSH、Stream、JSON、Search、Vector、Profiler、Slow Log、Pub/Sub 和插件系统不进入本次交付闭环。
- Browser 必须使用 `SCAN` 分页，不得使用阻塞性的 `KEYS`。
- 连接元数据写入应用数据目录 JSON；密码只写入系统钥匙串，不得写入普通配置文件、日志或前端 localStorage。
- 不引入 Redis Cloud/Azure/其他云 SDK、云 API、云登录、云数据库发现、云账户模型或 Cloud feature flag。
- 不引入 SQL 数据库；所有持久化使用 JSON 文件和系统钥匙串；禁止在循环遍历中查询 SQL。
- Rust commands 使用稳定的 snake_case 名称和可序列化 DTO；前端只通过 `@tauri-apps/api` 的 `invoke` 调用。
- 每个功能按 TDD 执行：先写一个会正确失败的测试，确认失败原因，再写最小实现，确认通过后重构。
- 所有提交信息使用简体中文；默认在当前 `main` 分支工作，不创建功能分支。
- 最终必须执行并记录 Rust 单元测试、Redis 集成测试、前端测试、前端生产构建和 Tauri 构建检查。

## File Map

### 工程与前端基础设施

- `package.json`：npm scripts 和前端/Tauri 依赖。
- `tsconfig.json`、`tsconfig.node.json`：TypeScript 编译边界。
- `vite.config.ts`：React 插件、Tauri 开发端口 1420 和 HMR 配置。
- `vitest.config.ts`、`src/test/setup.ts`：前端测试环境和 DOM matchers。
- `index.html`、`src/main.tsx`、`src/App.tsx`、`src/styles.css`：应用入口和基础主题。
- `src/lib/types.ts`：与 Rust DTO 一致的前端类型。
- `src/lib/tauri.ts`：所有 IPC wrapper 的唯一入口。

### Rust 核心

- `src-tauri/Cargo.toml`、`src-tauri/build.rs`、`src-tauri/tauri.conf.json`：Tauri 工程和依赖。
- `src-tauri/capabilities/default.json`：最小桌面权限。
- `src-tauri/src/lib.rs`：Tauri builder、`AppState` 和 command 注册。
- `src-tauri/src/error.rs`：稳定的错误码和安全消息。
- `src-tauri/src/domain/`：连接、键和值、命令输入输出 DTO。
- `src-tauri/src/persistence/`：JSON profile repository 和系统 keyring adapter。
- `src-tauri/src/redis/`：Redis client、连接生命周期、SCAN、CRUD、TTL 和 Workbench parser。
- `src-tauri/src/commands/`：薄 Tauri command 层。
- `src-tauri/tests/`：跨模块 Rust 单元/集成测试。
- `src-tauri/tests/support/mod.rs`：跨 Rust integration test 共用的 profile 和 fake store helper。

### 产品功能

- `src/features/connections/`：连接列表、连接表单、测试/打开/删除流程。
- `src/features/browser/`：键过滤、分页列表、键详情和五种数据类型编辑器。
- `src/features/workbench/`：命令输入、历史记录、结果面板。
- `docs/non-cloud-scope.md`：明确排除项和人工审查清单。
- `scripts/check-non-cloud-scope.mjs`：扫描源码、依赖和菜单文案中的 Cloud 入口。
- `README.md`：安装、开发、测试、本地 Redis 前置条件和功能范围。

---

### Task 1: 初始化 Tauri + React/Vite 工程

**Files:**
- Create: `package.json`
- Create: `tsconfig.json`
- Create: `tsconfig.node.json`
- Create: `vite.config.ts`
- Create: `vitest.config.ts`
- Create: `index.html`
- Create: `src/test/setup.ts`
- Create: `src/app.smoke.test.tsx`
- Create: `src/main.tsx`
- Create: `src/App.tsx`
- Create: `src/styles.css`
- Create: `src-tauri/Cargo.toml`
- Create: `src-tauri/build.rs`
- Create: `src-tauri/tauri.conf.json`
- Create: `src-tauri/capabilities/default.json`
- Create: `src-tauri/icons/icon.svg`
- Generate: `src-tauri/icons/icon.png` and the platform icon assets from the SVG source
- Create: `src-tauri/src/lib.rs`
- Create: `.gitignore`

**Interfaces:**
- Produces npm scripts `dev`, `build`, `test`, `test:watch`, `tauri:dev`, `tauri:build`.
- Produces a Tauri window named `main` at `http://localhost:1420` with default size `1480x960` and minimum size `1240x760`.
- Produces a React root that renders the text `Redix` and a default workspace heading.

- [ ] **Step 1: 写基线失败测试**

```tsx
// src/app.smoke.test.tsx
import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import App from "./App";

describe("Redix 应用壳", () => {
  it("显示应用名称和默认工作区", () => {
    render(<App />);

    expect(screen.getByRole("heading", { name: "Redix" })).toBeInTheDocument();
    expect(screen.getByText("Browser")).toBeInTheDocument();
    expect(screen.getByText("Workbench")).toBeInTheDocument();
  });
});
```

- [ ] **Step 2: 安装依赖并确认测试因 App 尚未实现而失败**

运行：

```bash
npm install
npm test -- --run src/app.smoke.test.tsx
```

预期：Vitest 启动成功，但测试失败，原因是 `App` 尚未渲染名为 `Redix` 的 heading 或工作区标签；失败不能来自缺少测试环境或语法错误。

- [ ] **Step 3: 写最小工程配置和应用壳**

`package.json` 使用以下脚本和依赖边界：

```json
{
  "scripts": {
    "dev": "vite",
    "build": "tsc -b && vite build",
    "test": "vitest run",
    "test:watch": "vitest",
    "tauri:dev": "tauri dev",
    "tauri:build": "tauri build"
  },
  "dependencies": {
    "@tauri-apps/api": "^2.11.1",
    "react": "^19.2.7",
    "react-dom": "^19.2.7"
  },
  "devDependencies": {
    "@tauri-apps/cli": "^2.11.4",
    "@testing-library/jest-dom": "^6.9.1",
    "@testing-library/react": "^16.3.2",
    "@types/react": "^19.2.16",
    "@types/react-dom": "^19.2.3",
    "@vitejs/plugin-react": "^4.7.0",
    "jsdom": "^29.1.1",
    "typescript": "~5.8.3",
    "vite": "^7.3.5",
    "vitest": "^4.1.8"
  }
}
```

`src/App.tsx` 只实现可测试的 shell；不要在这个任务中加入 Redis 逻辑。`vite.config.ts` 使用 Tauri 默认的 `1420` dev server 和 `1421` HMR 端口。`src-tauri/Cargo.toml` 先加入 Tauri、`serde`、`serde_json` 和 `thiserror`，Redis/keyring 依赖在对应任务加入。创建最小的 `src-tauri/icons/icon.svg`，并在本任务中运行 Tauri CLI 图标生成命令，确保 `src-tauri/icons/icon.png` 在 Rust 编译和测试前已经存在；Task 10 只负责正式 bundle 配置引用这些已生成资源。`src-tauri/src/lib.rs` 先注册空的 `invoke_handler` 并返回可运行的 Tauri builder。

- [ ] **Step 4: 运行基线测试和前端构建**

运行：

```bash
npm test -- --run src/app.smoke.test.tsx
npm run build
```

预期：测试通过，构建生成 `dist/`，无 TypeScript 错误。

- [ ] **Step 5: 提交**

```bash
git add package.json package-lock.json tsconfig.json tsconfig.node.json vite.config.ts vitest.config.ts index.html src src-tauri .gitignore
git commit -m "初始化 Tauri React 工程"
```

---

### Task 2: 建立领域模型和稳定错误类型

**Files:**
- Create: `src-tauri/src/error.rs`
- Create: `src-tauri/src/domain/mod.rs`
- Create: `src-tauri/src/domain/profile.rs`
- Create: `src-tauri/src/domain/key.rs`
- Create: `src-tauri/src/domain/workbench.rs`
- Create: `src-tauri/tests/domain.rs`
- Create: `src-tauri/tests/support/mod.rs`
- Modify: `src-tauri/src/lib.rs`

**Interfaces:**

```rust
// src-tauri/src/domain/profile.rs
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct ConnectionProfile {
    pub id: String,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub username: Option<String>,
    pub database: u8,
    pub has_password: bool,
}

impl ConnectionProfile {
    pub fn validate(&self) -> Result<(), crate::error::AppError>;
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SaveConnectionInput {
    pub profile: ConnectionProfile,
    pub password: Option<String>,
}

pub type TestConnectionInput = SaveConnectionInput;
```

```rust
// src-tauri/src/domain/key.rs
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ScanKeysInput {
    pub connection_id: String,
    pub cursor: u64,
    pub pattern: String,
    pub count: usize,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct KeySummary {
    pub key: String,
    pub key_type: String,
    pub ttl_ms: i64,
    pub size: Option<u64>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct ScanPage {
    pub cursor: u64,
    pub keys: Vec<KeySummary>,
    pub has_more: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct ConnectionInfo {
    pub server_version: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GetKeyInput {
    pub connection_id: String,
    pub key: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SetKeyInput {
    pub connection_id: String,
    pub key: String,
    pub value: RedisValue,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DeleteKeyInput {
    pub connection_id: String,
    pub key: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SetKeyTtlInput {
    pub connection_id: String,
    pub key: String,
    pub ttl_ms: i64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExecuteCommandInput {
    pub connection_id: String,
    pub command: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub enum RedisValue {
    String { value: String },
    Hash { fields: Vec<HashEntry> },
    List { items: Vec<String> },
    Set { members: Vec<String> },
    SortedSet { members: Vec<SortedSetEntry> },
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct HashEntry {
    pub field: String,
    pub value: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct SortedSetEntry {
    pub member: String,
    pub score: f64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct KeyValue {
    pub key: String,
    pub key_type: String,
    pub ttl_ms: i64,
    pub value: RedisValue,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct CommandResult {
    pub kind: String,
    pub value: serde_json::Value,
}
```

`AppError` 至少包含 `InvalidConnection`、`ConnectionFailed`、`AuthenticationFailed`、`UnsupportedDataType`、`CommandFailed` 和 `PersistenceFailed`，序列化为 `{ "code": string, "message": string }`。错误消息不得包含密码或完整 Redis URI。

- [ ] **Step 1: 写失败测试**

```rust
mod support;

use support::valid_profile;

#[test]
fn rejects_empty_host_zero_port_and_database_above_fifteen() {
    let profile = ConnectionProfile {
        id: "local".into(),
        name: "Local".into(),
        host: "".into(),
        port: 0,
        username: None,
        database: 16,
        has_password: false,
    };

    let error = profile.validate().expect_err("invalid profile must fail");
    assert_eq!(error.code(), "INVALID_CONNECTION");
}

#[test]
fn serializes_profile_without_password_field() {
    let profile = valid_profile();
    let json = serde_json::to_value(&profile).unwrap();
    let object = json.as_object().unwrap();

    assert!(!object.contains_key("password"));
    assert!(object.contains_key("has_password"));
}
```

- [ ] **Step 2: 运行领域测试确认正确失败**

运行：

```bash
cargo test --manifest-path src-tauri/Cargo.toml --test domain
```

预期：测试编译后因 `ConnectionProfile::validate` 和 `AppError::code` 尚未实现而失败；不能通过删除断言或放宽校验来让测试通过。

- [ ] **Step 3: 实现最小领域模型**

实现 `ConnectionProfile::validate`：去除首尾空白后要求 `id`、`name`、`host` 非空，要求 `port > 0`，要求 `database <= 15`；实现 `ScanKeysInput::validate`，要求 `count` 在 `1..=500`。定义 `RedisValue`、`HashEntry`、`SortedSetEntry`、`KeyValue` 和 Workbench `CommandResult` DTO，并为所有跨 IPC 类型派生 `Serialize`/`Deserialize`。在 `src-tauri/tests/support/mod.rs` 放置跨 integration test 共用的 helper：

```rust
use redix_lib::domain::ConnectionProfile;

pub fn valid_profile() -> ConnectionProfile {
    ConnectionProfile {
        id: "local".into(),
        name: "Local".into(),
        host: "127.0.0.1".into(),
        port: 6379,
        username: None,
        database: 0,
        has_password: false,
    }
}

pub fn invalid_profile() -> ConnectionProfile {
    ConnectionProfile {
        host: "".into(),
        port: 0,
        database: 16,
        ..valid_profile()
    }
}
```

- [ ] **Step 4: 运行领域测试确认通过**

运行：

```bash
cargo test --manifest-path src-tauri/Cargo.toml --test domain
```

预期：所有领域测试通过，JSON 中没有 password 字段，错误码稳定为大写下划线格式。

- [ ] **Step 5: 提交**

```bash
git add src-tauri/src/error.rs src-tauri/src/domain src-tauri/tests/domain.rs src-tauri/src/lib.rs
git commit -m "建立 Redis 客户端领域模型"
```

---

### Task 3: 实现 JSON 配置持久化和系统钥匙串

**Files:**
- Create: `src-tauri/src/persistence/mod.rs`
- Create: `src-tauri/src/persistence/profile_store.rs`
- Create: `src-tauri/src/persistence/secret_store.rs`
- Create: `src-tauri/tests/persistence.rs`
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/src/lib.rs`

**Interfaces:**

```rust
pub trait ProfileRepository: Send + Sync {
    fn load(&self) -> Result<Vec<ConnectionProfile>, AppError>;
    fn save(&self, profiles: &[ConnectionProfile]) -> Result<(), AppError>;
}

pub trait SecretStore: Send + Sync {
    fn read(&self, connection_id: &str) -> Result<Option<String>, AppError>;
    fn write(&self, connection_id: &str, password: &str) -> Result<(), AppError>;
    fn delete(&self, connection_id: &str) -> Result<(), AppError>;
}

pub struct JsonProfileRepository {
    pub path: std::path::PathBuf,
}

pub struct SystemKeyring {
    pub service: String,
}
```

`JsonProfileRepository` 将数据保存为 `{ "profiles": [...] }`，写入临时文件后原子替换目标文件。`SystemKeyring` 使用 service `redix`、account `redix/<connection-id>`，启用 macOS native、Windows native 和 Linux Secret Service backend。生产实现不得把 secret 写入 JSON。

- [ ] **Step 1: 写失败测试**

```rust
mod support;

use support::valid_profile;

#[derive(Default)]
struct InMemorySecretStore {
    values: std::sync::Mutex<std::collections::HashMap<String, String>>,
}

#[test]
fn repository_round_trips_profiles_without_secret_data() {
    let dir = tempfile::tempdir().unwrap();
    let repository = JsonProfileRepository::new(dir.path().join("connections.json"));
    let profile = valid_profile();

    repository.save(&[profile.clone()]).unwrap();
    let loaded = repository.load().unwrap();

    assert_eq!(loaded, vec![profile]);
    let raw = std::fs::read_to_string(dir.path().join("connections.json")).unwrap();
    assert!(!raw.contains("password"));
}

#[test]
fn in_memory_secret_store_supports_write_read_and_delete() {
    let secrets = InMemorySecretStore::default();

    secrets.write("local", "secret").unwrap();
    assert_eq!(secrets.read("local").unwrap().as_deref(), Some("secret"));
    secrets.delete("local").unwrap();
    assert_eq!(secrets.read("local").unwrap(), None);
}
```

- [ ] **Step 2: 运行持久化测试确认正确失败**

运行：

```bash
cargo test --manifest-path src-tauri/Cargo.toml --test persistence
```

预期：因 repository、`InMemorySecretStore` 和 keyring adapter 尚未实现而失败；失败不应来自临时目录或权限错误。

- [ ] **Step 3: 实现 repository、测试 double 和 keyring adapter**

在 `Cargo.toml` 加入 `keyring = { version = "4.1", features = ["apple-native", "windows-native", "sync-secret-service"] }` 和 dev dependency `tempfile = "3"`。`JsonProfileRepository` 负责目录创建、空文件视为空列表、JSON 解析错误映射和原子写入；测试 double 只放在测试模块，不进入生产 command 路径。

- [ ] **Step 4: 运行测试并检查秘密不落盘**

运行：

```bash
cargo test --manifest-path src-tauri/Cargo.toml --test persistence
rg -n 'password|secret' src-tauri/src/persistence/profile_store.rs
```

预期：测试通过；profile store 只处理 `has_password`，不读取或写入密码值。

- [ ] **Step 5: 提交**

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/persistence src-tauri/tests/persistence.rs src-tauri/src/lib.rs
git commit -m "增加连接配置和钥匙串持久化"
```

---

### Task 4: 实现 Rust Redis 服务和连接生命周期

**Files:**
- Create: `src-tauri/src/redis/mod.rs`
- Create: `src-tauri/src/redis/connection_manager.rs`
- Create: `src-tauri/src/redis/key_ops.rs`
- Create: `src-tauri/src/redis/workbench.rs`
- Create: `src-tauri/tests/redis_integration.rs`
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/src/lib.rs`

**Interfaces:**

```rust
pub struct RedisService {
    profiles: std::sync::Arc<dyn ProfileRepository>,
    secrets: std::sync::Arc<dyn SecretStore>,
    active: std::sync::Arc<tokio::sync::RwLock<std::collections::HashMap<String, redis::Client>>>,
}

pub trait RedisOperations {
    async fn test_connection(
        &self,
        profile: &ConnectionProfile,
        password: Option<&str>,
    ) -> Result<ConnectionInfo, AppError>;

    async fn open_connection(&self, connection_id: &str)
        -> Result<ConnectionInfo, AppError>;
    async fn close_connection(&self, connection_id: &str)
        -> Result<(), AppError>;
    async fn scan_keys(&self, input: ScanKeysInput)
        -> Result<ScanPage, AppError>;
    async fn get_key(&self, connection_id: &str, key: &str)
        -> Result<KeyValue, AppError>;
    async fn set_key(&self, input: SetKeyInput)
        -> Result<KeyValue, AppError>;
    async fn delete_key(&self, connection_id: &str, key: &str)
        -> Result<(), AppError>;
    async fn set_key_ttl(&self, input: SetKeyTtlInput)
        -> Result<i64, AppError>;
    async fn execute_command(&self, connection_id: &str, input: &str)
        -> Result<CommandResult, AppError>;
}
```

`ConnectionManager` 维护 `RwLock<HashMap<String, redis::Client>>`。`open_connection` 使用 profile 和 keyring secret 构造 Redis URL，执行 `PING` 成功后保存 client；每个操作从 client 创建异步 multiplexed connection，避免共享可变连接引用。只接受 `redis://` 的 Standalone host/port/database 配置，不解析云 URL 或拓扑配置。

### 4.1 先写失败的单元测试

- [ ] **Step 1: 写命令 tokenizer 和类型转换测试**

```rust
#[test]
fn parses_quoted_workbench_argument() {
    let args = tokenize_command("SET greeting \"hello world\"").unwrap();

    assert_eq!(args, vec!["SET", "greeting", "hello world"]);
}

#[test]
fn rejects_unterminated_workbench_quote() {
    let error = tokenize_command("SET greeting \"hello").unwrap_err();

    assert_eq!(error.code(), "COMMAND_FAILED");
}

#[test]
fn maps_unknown_redis_type_to_unsupported_data_type() {
    let error = decode_key_value("vector", vec![]).unwrap_err();

    assert_eq!(error.code(), "UNSUPPORTED_DATA_TYPE");
}
```

- [ ] **Step 2: 运行单元测试确认正确失败**

运行：

```bash
cargo test --manifest-path src-tauri/Cargo.toml redis:: --lib
```

预期：因 tokenizer、类型转换函数不存在而失败。

- [ ] **Step 3: 实现 Redis client、SCAN、CRUD、TTL 和命令执行**

在 `Cargo.toml` 加入：

```toml
redis = { version = "1.5", features = ["tokio-comp"] }
tokio = { version = "1", features = ["sync"] }
```

`scan_keys` 必须调用 `SCAN cursor MATCH pattern COUNT count`，将每个 key 的 `TYPE`、`PTTL` 和对应类型的长度映射为 `KeySummary`；不得调用 `KEYS`。`get_key`/`set_key` 实现 String、Hash、List、Set、Sorted Set；未知类型返回 `UNSUPPORTED_DATA_TYPE`。`execute_command` 使用 tokenizer 得到命令和参数，再用 `redis::cmd` 执行并把 Redis 返回值转换为 `CommandResult`。

- [ ] **Step 4: 运行单元测试确认通过**

运行：

```bash
cargo test --manifest-path src-tauri/Cargo.toml redis:: --lib
```

预期：tokenizer、类型映射、参数校验和错误映射测试全部通过。

### 4.2 运行本地 Redis 集成测试

- [ ] **Step 5: 写集成测试**

`src-tauri/tests/redis_integration.rs` 使用环境变量 `REDIX_TEST_REDIS_URL`，覆盖以下可复现流程：连接 `PING`、写入并读取 String、Hash、List、Set、Sorted Set、`SCAN` 过滤、设置 TTL、删除 key、执行 `PING`。没有设置环境变量时测试使用 `#[ignore]` 并输出启动提示，不能静默通过。

- [ ] **Step 6: 启动本地 Redis 并运行集成测试**

运行：

```bash
redis-server --daemonize yes
REDIX_TEST_REDIS_URL=redis://127.0.0.1:6379 cargo test --manifest-path src-tauri/Cargo.toml --test redis_integration -- --nocapture
```

预期：所有集成测试通过；失败时记录具体 Redis 命令和错误码，不打印密码。

- [ ] **Step 7: 提交**

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/redis src-tauri/tests/redis_integration.rs src-tauri/src/lib.rs
git commit -m "实现 Redis 连接和数据操作服务"
```

---

### Task 5: 接入 Tauri commands 和应用状态

**Files:**
- Create: `src-tauri/src/commands/mod.rs`
- Create: `src-tauri/src/commands/connections.rs`
- Create: `src-tauri/src/commands/browser.rs`
- Create: `src-tauri/src/commands/workbench.rs`
- Create: `src-tauri/tests/commands.rs`
- Modify: `src-tauri/src/lib.rs`

**Interfaces:**

```rust
#[tauri::command]
pub async fn list_connections(state: tauri::State<'_, AppState>)
    -> Result<Vec<ConnectionProfile>, AppError>;

#[tauri::command]
pub async fn save_connection(
    state: tauri::State<'_, AppState>,
    input: SaveConnectionInput,
) -> Result<ConnectionProfile, AppError>;

pub(crate) async fn save_connection_inner(
    state: &AppState,
    input: SaveConnectionInput,
) -> Result<ConnectionProfile, AppError>;

#[tauri::command]
pub async fn delete_connection(
    state: tauri::State<'_, AppState>,
    connection_id: String,
) -> Result<(), AppError>;

#[tauri::command]
pub async fn test_connection(
    state: tauri::State<'_, AppState>,
    input: TestConnectionInput,
) -> Result<ConnectionInfo, AppError>;

#[tauri::command]
pub async fn open_connection(
    state: tauri::State<'_, AppState>,
    connection_id: String,
) -> Result<ConnectionInfo, AppError>;

#[tauri::command]
pub async fn close_connection(
    state: tauri::State<'_, AppState>,
    connection_id: String,
) -> Result<(), AppError>;

#[tauri::command]
pub async fn scan_keys(
    state: tauri::State<'_, AppState>,
    input: ScanKeysInput,
) -> Result<ScanPage, AppError>;

#[tauri::command]
pub async fn get_key(
    state: tauri::State<'_, AppState>,
    input: GetKeyInput,
) -> Result<KeyValue, AppError>;

#[tauri::command]
pub async fn set_key(
    state: tauri::State<'_, AppState>,
    input: SetKeyInput,
) -> Result<KeyValue, AppError>;

#[tauri::command]
pub async fn delete_key(
    state: tauri::State<'_, AppState>,
    input: DeleteKeyInput,
) -> Result<(), AppError>;

#[tauri::command]
pub async fn set_key_ttl(
    state: tauri::State<'_, AppState>,
    input: SetKeyTtlInput,
) -> Result<i64, AppError>;

#[tauri::command]
pub async fn execute_command(
    state: tauri::State<'_, AppState>,
    input: ExecuteCommandInput,
) -> Result<CommandResult, AppError>;
```

- [ ] **Step 1: 写 command adapter 失败测试**

```rust
#[tokio::test]
async fn save_connection_rejects_invalid_profile_before_persistence() {
    let (state, secret_store) = test_app_state();
    let input = SaveConnectionInput {
        profile: invalid_profile(),
        password: Some("must-not-be-written".into()),
    };

    let error = save_connection_inner(&state, input).await.unwrap_err();

    assert_eq!(error.code(), "INVALID_CONNECTION");
    assert!(!secret_store.was_write_called());
}
```

- [ ] **Step 2: 运行测试确认正确失败**

运行：

```bash
cargo test --manifest-path src-tauri/Cargo.toml --test commands
```

预期：因 `AppState`、command adapter 和依赖注入尚未完成而失败。

- [ ] **Step 3: 实现 `AppState` 和薄 command 层**

`AppState` 持有 `Arc<dyn ProfileRepository>`、`Arc<dyn SecretStore>` 和 `RedisService`。`save_connection_inner` 先调用 `profile.validate()`，再写 secret 和 profile；如果 profile 无密码则删除旧 secret；Tauri wrapper `save_connection` 只把 `State` 借用转换为 `&AppState` 后委托给 inner 函数。`delete_connection` 先关闭 active client，再删除 profile 和 secret。command 测试使用 `test_app_state() -> (AppState, Arc<RecordingSecretStore>)`，其中 `RecordingSecretStore::was_write_called() -> bool` 可验证非法 profile 不写 secret。所有 command 只做输入校验、依赖调用和 DTO 返回，不在 command 中拼接 Redis 命令。

- [ ] **Step 4: 注册 commands 并运行测试**

在 `tauri::generate_handler![]` 注册全部 12 个 command：

```rust
tauri::generate_handler![
    list_connections,
    save_connection,
    delete_connection,
    test_connection,
    open_connection,
    close_connection,
    scan_keys,
    get_key,
    set_key,
    delete_key,
    set_key_ttl,
    execute_command,
]
```

运行：

```bash
cargo test --manifest-path src-tauri/Cargo.toml --test commands
cargo check --manifest-path src-tauri/Cargo.toml
```

预期：command 测试和 Rust 编译通过，错误 profile 不会触发 secret 写入。

- [ ] **Step 5: 提交**

```bash
git add src-tauri/src/commands src-tauri/src/lib.rs src-tauri/tests/commands.rs
git commit -m "接入 Tauri Redis commands"
```

---

### Task 6: 建立前端类型和 IPC bridge

**Files:**
- Create: `src/lib/types.ts`
- Create: `src/lib/tauri.ts`
- Create: `src/lib/tauri.test.ts`
- Modify: `src/App.tsx`

**Interfaces:**

```ts
export type Workspace = "browser" | "workbench";

export interface ConnectionProfile {
  id: string;
  name: string;
  host: string;
  port: number;
  username: string | null;
  database: number;
  has_password: boolean;
}

export interface SaveConnectionInput {
  profile: ConnectionProfile;
  password: string | null;
}

export interface ConnectionInfo {
  server_version: string;
}

export interface ScanKeysInput {
  connection_id: string;
  cursor: number;
  pattern: string;
  count: number;
}

export interface ScanPage {
  cursor: number;
  keys: KeySummary[];
  has_more: boolean;
}

export interface KeySummary {
  key: string;
  key_type: string;
  ttl_ms: number;
  size: number | null;
}

export type RedisValue =
  | { String: { value: string } }
  | { Hash: { fields: Array<{ field: string; value: string }> } }
  | { List: { items: string[] } }
  | { Set: { members: string[] } }
  | { SortedSet: { members: Array<{ member: string; score: number }> } };

export interface KeyValue {
  key: string;
  key_type: string;
  ttl_ms: number;
  value: RedisValue;
}

export interface GetKeyInput {
  connection_id: string;
  key: string;
}

export interface SetKeyInput {
  connection_id: string;
  key: string;
  value: RedisValue;
}

export interface DeleteKeyInput {
  connection_id: string;
  key: string;
}

export interface SetKeyTtlInput {
  connection_id: string;
  key: string;
  ttl_ms: number;
}

export interface ExecuteCommandInput {
  connection_id: string;
  command: string;
}

export interface CommandResult {
  kind: string;
  value: unknown;
}

export async function listConnections(): Promise<ConnectionProfile[]>;
export async function saveConnection(input: SaveConnectionInput): Promise<ConnectionProfile>;
export async function deleteConnection(connectionId: string): Promise<void>;
export async function testConnection(input: SaveConnectionInput): Promise<ConnectionInfo>;
export async function openConnection(connectionId: string): Promise<ConnectionInfo>;
export async function closeConnection(connectionId: string): Promise<void>;
export async function scanKeys(input: ScanKeysInput): Promise<ScanPage>;
export async function getKey(input: GetKeyInput): Promise<KeyValue>;
export async function setKey(input: SetKeyInput): Promise<KeyValue>;
export async function deleteKey(input: DeleteKeyInput): Promise<void>;
export async function setKeyTtl(input: SetKeyTtlInput): Promise<number>;
export async function executeCommand(input: ExecuteCommandInput): Promise<CommandResult>;
```

- [ ] **Step 1: 写 bridge 失败测试**

```ts
it("用稳定命令名调用 scan_keys", async () => {
  invokeMock.mockResolvedValue({ cursor: 0, keys: [], has_more: false });

  await scanKeys({ connection_id: "local", cursor: 0, pattern: "*", count: 100 });

  expect(invokeMock).toHaveBeenCalledWith("scan_keys", {
    input: { connection_id: "local", cursor: 0, pattern: "*", count: 100 },
  });
});
```

- [ ] **Step 2: 运行前端测试确认正确失败**

运行：

```bash
npm test -- --run src/lib/tauri.test.ts
```

预期：因 bridge exports 和 Tauri `invoke` mock 尚未实现而失败。

- [ ] **Step 3: 实现 typed wrappers 和错误归一化**

每个 wrapper 直接调用 `invoke<T>`，参数对象统一使用 Rust command 的 `input` 包装；捕获异常时保留 `{ code, message }`，不在前端拼接或暴露 secret。`src/lib/types.ts` 的字段名必须与 Rust serde 输出保持一致。

- [ ] **Step 4: 运行测试确认通过**

运行：

```bash
npm test -- --run src/lib/tauri.test.ts
npm run build
```

预期：所有 bridge 测试和前端编译通过。

- [ ] **Step 5: 提交**

```bash
git add src/lib/types.ts src/lib/tauri.ts src/lib/tauri.test.ts src/App.tsx
git commit -m "增加前端 Redis IPC bridge"
```

---

### Task 7: 实现连接管理页面

**Files:**
- Create: `src/features/connections/connectionState.ts`
- Create: `src/features/connections/ConnectionPage.tsx`
- Create: `src/features/connections/ConnectionList.tsx`
- Create: `src/features/connections/ConnectionForm.tsx`
- Create: `src/features/connections/connections.test.tsx`
- Modify: `src/App.tsx`
- Modify: `src/styles.css`

**Interfaces:**

- `ConnectionPage` 接收 `{ onOpenConnection: (profile: ConnectionProfile) => void }`。
- `ConnectionForm` 接收 `{ initial?: ConnectionProfile; onSaved: (profile: ConnectionProfile) => void; onCancel: () => void }`。
- 页面状态包括 `profiles`、`editingProfile`、`testingId`、`saving`、`error` 和 `activeId`，不可将密码放入全局持久化状态。

测试文件顶部使用 `vi.hoisted` 创建 `listConnectionsMock`、`saveConnectionMock`、`openConnectionMock`、`deleteConnectionMock`、`testConnectionMock` 和 `onOpenConnectionMock`，并通过 `vi.mock("../../lib/tauri", () => ({ ... }))` 替换 bridge。固定 fixture 为：

```ts
const localProfile: ConnectionProfile = {
  id: "local",
  name: "本地 Redis",
  host: "127.0.0.1",
  port: 6379,
  username: null,
  database: 0,
  has_password: false,
};
```

- [ ] **Step 1: 写连接页面失败测试**

```tsx
it("无连接时显示新增提示并能打开连接表单", async () => {
  listConnectionsMock.mockResolvedValue([]);
  render(<ConnectionPage onOpenConnection={vi.fn()} />);

  expect(await screen.findByText("还没有 Redis 连接")).toBeInTheDocument();
  await userEvent.click(screen.getByRole("button", { name: "新增连接" }));

  expect(screen.getByLabelText("连接名称")).toBeInTheDocument();
  expect(screen.getByLabelText("主机")).toBeInTheDocument();
  expect(screen.getByLabelText("端口")).toBeInTheDocument();
});

it("保存成功后刷新连接列表并打开对应连接", async () => {
  listConnectionsMock.mockResolvedValue([]);
  saveConnectionMock.mockResolvedValue(localProfile);
  openConnectionMock.mockResolvedValue({ server_version: "8.4.0" });
  render(<ConnectionPage onOpenConnection={onOpenConnectionMock} />);

  await userEvent.click(screen.getByRole("button", { name: "新增连接" }));
  await fillStandaloneForm();
  await userEvent.click(screen.getByRole("button", { name: "保存并连接" }));

  expect(saveConnectionMock).toHaveBeenCalled();
  expect(openConnectionMock).toHaveBeenCalledWith(localProfile.id);
  expect(onOpenConnectionMock).toHaveBeenCalledWith(localProfile);
});

async function fillStandaloneForm() {
  await userEvent.type(screen.getByLabelText("连接名称"), "本地 Redis");
  await userEvent.clear(screen.getByLabelText("主机"));
  await userEvent.type(screen.getByLabelText("主机"), "127.0.0.1");
  await userEvent.clear(screen.getByLabelText("端口"));
  await userEvent.type(screen.getByLabelText("端口"), "6379");
}
```

- [ ] **Step 2: 运行测试确认正确失败**

运行：

```bash
npm test -- --run src/features/connections/connections.test.tsx
```

预期：因页面组件和状态逻辑尚未存在而失败。

- [ ] **Step 3: 实现连接状态和表单**

表单只提供 name、host、port、username、database、password；默认 host `127.0.0.1`、port `6379`、database `0`。保存前校验 name/host/port/database，使用 `crypto.randomUUID()` 为新 profile 生成 id。测试连接只调用 `testConnection`，保存并连接先调用 `saveConnection` 再调用 `openConnection`。密码字段使用 `type="password"`，编辑已有 profile 时为空表示保留原 secret。

- [ ] **Step 4: 实现列表、删除和错误提示并运行测试**

连接列表显示名称、`host:port`、database 和连接状态；删除前使用浏览器 `window.confirm`，删除成功后清理 active profile。错误提示使用可访问的 `role="alert"`，不渲染原始命令或密码。

运行：

```bash
npm test -- --run src/features/connections/connections.test.tsx
npm run build
```

预期：连接页面测试和前端构建通过。

- [ ] **Step 5: 提交**

```bash
git add src/features/connections src/App.tsx src/styles.css
git commit -m "实现 Redis 连接管理页面"
```

---

### Task 8: 实现 Browser 和五种数据类型编辑器

**Files:**
- Create: `src/features/browser/browserState.ts`
- Create: `src/features/browser/BrowserPage.tsx`
- Create: `src/features/browser/KeyList.tsx`
- Create: `src/features/browser/KeyDetails.tsx`
- Create: `src/features/browser/KeyEditor.tsx`
- Create: `src/features/browser/browser.test.tsx`
- Modify: `src/App.tsx`
- Modify: `src/styles.css`

**Interfaces:**

- `BrowserPage` 接收 `{ connectionId: string }`，内部维护 `pattern`、`cursor`、`keys`、`selectedKey`、`detail`、`loading` 和 `error`。
- `KeyList` 使用 `scanKeys`，每次过滤模式变化都将 cursor 重置为 0 并替换 keys；点击“加载更多”使用上一次返回 cursor。
- `KeyDetails` 通过 `getKey` 按需读取值，编辑成功后重新读取详情；删除成功后从列表移除 key。
- 五种编辑器分别产生 `RedisValue`：String `{ value }`、Hash `{ fields }`、List `{ items }`、Set `{ members }`、Sorted Set `{ members: [{ member, score }] }`。

测试文件通过 `vi.mock("../../lib/tauri", () => ({ ... }))` 提供 `scanKeysMock`、`getKeyMock`、`setKeyMock`、`deleteKeyMock` 和 `setKeyTtlMock`；每个 mock 使用 `vi.fn()`，默认返回 Promise，测试用例按需覆盖 `mockResolvedValue`。

- [ ] **Step 1: 写 Browser 失败测试**

```tsx
it("按模式加载键并在点击键后读取详情", async () => {
  scanKeysMock.mockResolvedValue({
    cursor: 0,
    keys: [{ key: "user:1", key_type: "string", ttl_ms: -1, size: 5 }],
    has_more: false,
  });
  getKeyMock.mockResolvedValue({ key: "user:1", key_type: "string", ttl_ms: -1, value: { String: { value: "Alice" } } });
  render(<BrowserPage connectionId="local" />);

  expect(await screen.findByText("user:1")).toBeInTheDocument();
  await userEvent.click(screen.getByText("user:1"));

  expect(await screen.findByDisplayValue("Alice")).toBeInTheDocument();
  expect(getKeyMock).toHaveBeenCalledWith({ connection_id: "local", key: "user:1" });
});

it("保存 String 后刷新详情", async () => {
  scanKeysMock.mockResolvedValue({
    cursor: 0,
    keys: [{ key: "user:1", key_type: "string", ttl_ms: -1, size: 5 }],
    has_more: false,
  });
  getKeyMock
    .mockResolvedValueOnce({ key: "user:1", key_type: "string", ttl_ms: -1, value: { String: { value: "Alice" } } })
    .mockResolvedValueOnce({ key: "user:1", key_type: "string", ttl_ms: -1, value: { String: { value: "Bob" } } });
  setKeyMock.mockResolvedValue({ key: "user:1", key_type: "string", ttl_ms: -1, value: { String: { value: "Bob" } } });
  render(<BrowserPage connectionId="local" />);

  await userEvent.click(await screen.findByText("user:1"));
  const input = await screen.findByDisplayValue("Alice");
  await userEvent.clear(input);
  await userEvent.type(input, "Bob");
  await userEvent.click(screen.getByRole("button", { name: "保存" }));

  expect(setKeyMock).toHaveBeenCalledWith({
    connection_id: "local",
    key: "user:1",
    value: { String: { value: "Bob" } },
  });
  expect(getKeyMock).toHaveBeenCalledTimes(2);
});
```

- [ ] **Step 2: 运行测试确认正确失败**

运行：

```bash
npm test -- --run src/features/browser/browser.test.tsx
```

预期：因 Browser 页面、列表和编辑器尚未实现而失败。

- [ ] **Step 3: 实现 SCAN 列表和详情布局**

Browser 页面使用左键列表/右详情的两栏布局；过滤输入默认 `*`，回车或 debounce 触发 `scanKeys`。展示 key、类型、TTL；游标非 0 时显示“加载更多”。请求进行中禁用重复按钮，错误显示 `role="alert"`。不在 React 中执行 `KEYS` 或模拟扫描。

- [ ] **Step 4: 实现 String、Hash、List、Set、Sorted Set 编辑器**

String 使用单个 textarea；Hash 使用 field/value 行编辑；List 使用有序 item 行；Set 使用 member 行并在提交前去重；Sorted Set 使用 member/score 行并校验 score 为有限数字。每个编辑器提供保存、删除和 TTL 设置，调用 `setKey`/`deleteKey`/`setKeyTtl`，成功后重新调用 `getKey`。

- [ ] **Step 5: 运行 Browser 测试、构建和可访问性检查**

运行：

```bash
npm test -- --run src/features/browser/browser.test.tsx
npm run build
```

预期：Browser 测试覆盖列表、详情、String 保存、删除、过滤重置和错误提示，前端构建通过。

- [ ] **Step 6: 提交**

```bash
git add src/features/browser src/App.tsx src/styles.css
git commit -m "实现 Redis Browser 和键编辑"
```

---

### Task 9: 实现 Workbench 命令工作区

**Files:**
- Create: `src/features/workbench/workbenchState.ts`
- Create: `src/features/workbench/WorkbenchPage.tsx`
- Create: `src/features/workbench/CommandInput.tsx`
- Create: `src/features/workbench/CommandResult.tsx`
- Create: `src/features/workbench/workbench.test.tsx`
- Modify: `src/App.tsx`
- Modify: `src/styles.css`

**Interfaces:**

- `WorkbenchPage` 接收 `{ connectionId: string | null }`。
- 输入为空或没有 active connection 时禁用执行按钮。
- 提交 payload 为 `{ connection_id: string, command: string }`，结果保存为 `{ command, result, created_at }` 的本地内存历史，不持久化密码或完整连接 URI。
- 结果渲染标量、列表、嵌套数组和错误；大结果在 UI 中折叠，不截断 Rust 返回值。

测试文件通过 `vi.mock("../../lib/tauri", () => ({ executeCommand: executeCommandMock }))` 注入 `const executeCommandMock = vi.fn()`，并用 `vi.clearAllMocks()` 在每个测试前清理调用记录。

- [ ] **Step 1: 写 Workbench 失败测试**

```tsx
it("没有连接或命令为空时禁用执行", () => {
  render(<WorkbenchPage connectionId={null} />);

  expect(screen.getByRole("button", { name: "执行" })).toBeDisabled();
});

it("执行命令并展示结果", async () => {
  executeCommandMock.mockResolvedValue({ kind: "string", value: "PONG" });
  render(<WorkbenchPage connectionId="local" />);

  await userEvent.type(screen.getByRole("textbox", { name: "Redis 命令" }), "PING");
  await userEvent.click(screen.getByRole("button", { name: "执行" }));

  expect(executeCommandMock).toHaveBeenCalledWith({ connection_id: "local", command: "PING" });
  expect(await screen.findByText("PONG")).toBeInTheDocument();
});
```

- [ ] **Step 2: 运行测试确认正确失败**

运行：

```bash
npm test -- --run src/features/workbench/workbench.test.tsx
```

预期：因 Workbench 组件和结果状态尚未实现而失败。

- [ ] **Step 3: 实现命令输入、结果和历史记录**

支持 Cmd/Ctrl+Enter 执行，执行中显示 loading 并禁用按钮；成功结果显示结构化 JSON/文本，失败结果显示稳定错误码和 message。历史记录按最近执行在前排列，点击历史可以回填输入但不会自动执行。

- [ ] **Step 4: 运行 Workbench 测试和构建**

运行：

```bash
npm test -- --run src/features/workbench/workbench.test.tsx
npm run build
```

预期：WorkBench 测试覆盖禁用状态、执行 payload、结果展示、错误展示和历史回填，构建通过。

- [ ] **Step 5: 提交**

```bash
git add src/features/workbench src/App.tsx src/styles.css
git commit -m "实现 Redis Workbench 工作区"
```

---

### Task 10: 完成主题、非 Cloud 审查、文档和最终验证

**Files:**
- Create: `docs/non-cloud-scope.md`
- Create: `scripts/check-non-cloud-scope.mjs`
- Create: `scripts/check-non-cloud-scope.test.mjs`
- Modify: `src/styles.css`
- Modify: `src-tauri/tauri.conf.json`
- Modify: `README.md`
- Modify: `package.json`
- Modify: `task_plan.md`
- Modify: `findings.md`
- Modify: `progress.md`

**Interfaces:**
- `npm run check:non-cloud` 扫描 `src/`、`src-tauri/src/`、`package.json` 和菜单文案，命中 `redis cloud`、`redis-cloud`、`cloud-capi`、`azure managed redis`、`cloud login` 等排除词时返回非零状态；脚本自身不扫描设计文档中的范围说明。
- `scanFiles(contents: string[]): string[]` 返回按出现顺序去重的排除词命中列表；`main()` 读取固定产品目录并在命中时打印文件和词后退出码为 1。
- `docs/non-cloud-scope.md` 列出明确排除项、允许的本地 Redis Standalone 能力和人工审查结果。
- `README.md` 提供 `npm install`、`npm run dev`、`npm run tauri:dev`、测试命令、本地 Redis 前置条件和当前 MVP 边界。

- [ ] **Step 1: 写非 Cloud 审查失败测试**

```js
// scripts/check-non-cloud-scope.test.mjs
import { describe, expect, it } from "vitest";
import { scanFiles } from "./check-non-cloud-scope.mjs";

describe("非 Cloud 范围检查", () => {
  it("没有排除功能入口时通过", () => {
    expect(scanFiles(["src/App.tsx", "src-tauri/src/lib.rs"].map(() => "local redis"))).toEqual([]);
  });

  it("发现 Cloud 入口时返回命中词", () => {
    expect(scanFiles(["redis cloud login"])).toContain("redis cloud");
  });
});
```

- [ ] **Step 2: 运行审查测试确认正确失败**

运行：

```bash
npm test -- --run scripts/check-non-cloud-scope.test.mjs
```

预期：因脚本和 `scanFiles` 尚未实现而失败。

- [ ] **Step 3: 实现检查脚本、主题和图标**

脚本只接受文本数组并返回命中的排除词，生产扫描入口使用 `rg --files` 得到 `src/`、`src-tauri/src/` 和 `package.json` 文件。更新 `styles.css` 提供深色默认主题、浅色主题类、焦点态和 `role="alert"` 对应样式。在 `tauri.conf.json` 设置产品名 `Redix`、identifier `com.redix.desktop`、窗口大小，并将 bundle icon 配置指向 Task 1 已生成的图标资源。

- [ ] **Step 4: 更新 README 和 npm scripts**

在 `package.json` 增加：

```json
{
  "scripts": {
    "check:non-cloud": "node scripts/check-non-cloud-scope.mjs",
    "test:frontend": "vitest run",
    "test:rust": "cargo test --manifest-path src-tauri/Cargo.toml"
  }
}
```

README 明确说明：Redix 是本地 Redis Standalone 管理工具；密码使用系统钥匙串；Browser 使用 SCAN；Redis Cloud、Azure Managed Redis、Cluster、Sentinel、TLS、SSH 和模块功能不属于当前版本。

- [ ] **Step 5: 运行完整验证**

运行：

```bash
npm run check:non-cloud
npm run test:frontend
npm run build
cargo test --manifest-path src-tauri/Cargo.toml
REDIX_TEST_REDIS_URL=redis://127.0.0.1:6379 cargo test --manifest-path src-tauri/Cargo.toml --test redis_integration -- --nocapture
npm run tauri:build
git diff --check
git status --short
```

预期：非 Cloud 检查、前端测试、前端构建、Rust 测试、Redis 集成测试和 Tauri 构建均返回 0；`git diff --check` 无输出；工作区只包含本任务的预期变更。任何失败都要先记录到 `progress.md`，按失败原因修正后重新执行对应完整命令。

- [ ] **Step 6: 更新规划记录并提交**

```bash
git add README.md docs/non-cloud-scope.md scripts src src-tauri package.json package-lock.json task_plan.md findings.md progress.md
git commit -m "完成 Redix Redis 客户端 MVP"
```

---

## Self-Review Checklist

- [x] 设计文档中的 MVP 功能均有 Task 1-10 覆盖：Standalone 连接、持久化、SCAN Browser、五种类型 CRUD、TTL、Workbench、主题、测试和构建。
- [x] Redis Cloud、Azure 和云服务入口在 Global Constraints、Task 10 和 `docs/non-cloud-scope.md` 中明确排除。
- [x] 没有引入 SQL，也没有任何循环 SQL 查询路径。
- [x] Rust 类型名称与前端 DTO 名称保持一致：`ConnectionProfile`、`SaveConnectionInput`、`ScanKeysInput`、`ScanPage`、`KeyValue`、`CommandResult`。
- [x] 连接生命周期接口完整包含 `test_connection`、`open_connection`、`close_connection`。
- [x] 每个产品功能任务都先写失败测试、验证失败、实现、验证通过，再提交。
- [x] 没有使用模糊占位步骤。
