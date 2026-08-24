# 连接配置导入导出与 Standalone TLS/证书实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在 Redix 中补齐 Standalone TLS/证书连接能力和不携带 secrets 的连接配置导入导出，并保持现有连接、Browser、Workbench、分析、监控、Pub/Sub 与 Profiler 行为不回归。

**Architecture:** 扩展现有 `ConnectionProfile` 保存 TLS 元数据，使用结构化系统钥匙串保存密码、CA PEM、客户端证书和私钥；在 `RedisService` 中建立唯一 TLS-aware client builder，所有 Redis 连接派生路径共用。连接传输使用 Rust 端版本化 JSON 归一化和批量持久化，前端仅负责文件选择、下载和结果呈现。

**Tech Stack:** Rust 2021、Tauri 2、`redis` 1.5 Rustls、Serde、系统钥匙串、React 19、TypeScript、Vitest、Testing Library。

**Spec:** `docs/superpowers/specs/2026-08-24-connection-import-export-standalone-tls-design.md`

## Global Constraints

- 继续在当前 `main` 分支开发，不创建分支或工作树。
- 提交信息使用简体中文。
- 普通连接导出绝不包含密码、CA PEM、客户端证书或私钥；导入文件中的 secrets 只统计并忽略。
- 只实现 Standalone TCP/TLS；不实现 SSH、Sentinel、Cluster、Redis Cloud、独立证书管理器、证书文件路径读取或 SQL。
- 所有证书材料只通过表单 PEM 录入并保存到钥匙串，不能写入 profile JSON、导出 JSON 或 IPC 返回 profile。
- 先写失败测试并确认 RED，再写最小实现；每个任务完成后运行该任务的定向测试并提交。
- 禁止在循环遍历中查询 SQL；本功能不引入 SQL 查询。
- 使用 `apply_patch` 修改本地文件；完成前必须提供新鲜的 Rust、前端、构建、范围和格式验证证据。

## 文件与职责映射

- `src-tauri/src/domain/profile.rs`：连接 TLS 元数据、保存/测试输入、PEM 校验和默认值。
- `src-tauri/src/domain/connection_transfer.rs`：Redix v1 导出 DTO、导入 DTO、别名归一化和逐条校验。
- `src-tauri/src/domain/mod.rs`：导出新领域类型。
- `src-tauri/src/persistence/secret_store.rs`、`src-tauri/src/persistence/mod.rs`：结构化 `ConnectionSecrets`、历史纯密码兼容、钥匙串读写。
- `src-tauri/src/commands/connections.rs`：保存 secrets 合并/回滚、导入导出 command、测试连接 secrets 解析。
- `src-tauri/src/commands/mod.rs`、`src-tauri/src/lib.rs`：导出并注册新增 commands。
- `src-tauri/src/redis/connection_manager.rs`：TLS URL、`TlsCertificates`、统一 client builder 和所有连接路径迁移。
- `src-tauri/Cargo.toml`、`src-tauri/Cargo.lock`：Rustls 和 UUID feature/dependency。
- `src-tauri/tests/support/mod.rs`、`src-tauri/tests/persistence.rs`、`src-tauri/tests/commands.rs`：更新内存 secrets mock、持久化/IPC 测试。
- `src/lib/types.ts`、`src/lib/tauri.ts`：typed profile、保存输入、传输 DTO 和 invoke bridge。
- `src/features/connections/connectionState.ts`：TLS 表单状态、默认值和输入转换。
- `src/features/connections/ConnectionForm.tsx`：TLS/证书表单、前端 PEM 提示、保存/测试入参。
- `src/features/connections/ConnectionList.tsx`：TLS 与证书待录入状态展示。
- `src/features/connections/ConnectionPage.tsx`、`src/styles.css`：导入导出文件交互、结果反馈和布局。
- `src/features/connections/connections.test.tsx`：连接表单、TLS、导入导出和现有行为回归。

---

### Task 1: 扩展连接领域模型与结构化钥匙串

**Files:**
- Modify: `src-tauri/src/domain/profile.rs`
- Modify: `src-tauri/src/persistence/secret_store.rs`
- Modify: `src-tauri/src/persistence/mod.rs`
- Modify: `src-tauri/src/domain/mod.rs`
- Modify: `src-tauri/tests/support/mod.rs`
- Modify: `src-tauri/tests/persistence.rs`
- Modify: `src-tauri/src/commands/connections.rs` 的现有测试 mock
- Modify: `src-tauri/src/redis/connection_manager.rs` 的现有测试 mock

**Interfaces:**
- Produces `ConnectionProfile { tls, verify_server_cert, ca_certificate_name, client_certificate_name, has_ca_certificate, has_client_certificate }`。
- Produces `ConnectionSecrets { password, ca_certificate, client_certificate, client_key }`，以及 `is_empty()`、`has_client_certificate()`。
- Changes `SecretStore` to `read(&str) -> Result<Option<ConnectionSecrets>, AppError>` and `write(&str, &ConnectionSecrets) -> Result<(), AppError>`。
- Extends `SaveConnectionInput`/`TestConnectionInput` with optional certificate bodies and `clear_ca_certificate`/`clear_client_certificate` defaulting to false。

- [ ] **Step 1: Write the failing domain and secret tests**

```rust
#[test]
fn legacy_profile_defaults_to_plain_redis_and_verified_tls() {
    let profile: ConnectionProfile = serde_json::from_value(serde_json::json!({
        "id": "local", "name": "Local", "host": "127.0.0.1",
        "port": 6379, "username": null, "database": 0, "has_password": false
    })).unwrap();

    assert!(!profile.tls);
    assert!(profile.verify_server_cert);
    assert!(!profile.has_ca_certificate);
    assert!(!profile.has_client_certificate);
}

#[test]
fn structured_secret_store_accepts_legacy_raw_password() {
    let secrets = decode_stored_secret("old-password").unwrap();
    assert_eq!(secrets.password.as_deref(), Some("old-password"));
    assert!(secrets.ca_certificate.is_none());
}

#[test]
fn pem_validation_requires_certificate_or_private_key_markers() {
    assert!(validate_certificate_pem("-----BEGIN CERTIFICATE-----x-----END CERTIFICATE-----").is_ok());
    assert!(validate_private_key_pem("-----BEGIN RSA PRIVATE KEY-----x-----END RSA PRIVATE KEY-----").is_ok());
    assert!(validate_certificate_pem("not-pem").is_err());
}
```

- [ ] **Step 2: Run the focused tests and confirm RED**

Run: `cargo test --manifest-path src-tauri/Cargo.toml legacy_profile_defaults_to_plain_redis_and_verified_tls structured_secret_store_accepts_legacy_raw_password pem_validation_requires_certificate_or_private_key_markers`

Expected: FAIL because the TLS fields, structured secrets and PEM helpers do not yet exist.

- [ ] **Step 3: Implement the model and backward-compatible keyring codec**

Implement the following semantics:

```rust
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ConnectionSecrets {
    pub password: Option<String>,
    pub ca_certificate: Option<String>,
    pub client_certificate: Option<String>,
    pub client_key: Option<String>,
}

impl SecretStore for SystemKeyring {
    fn read(&self, id: &str) -> Result<Option<ConnectionSecrets>, AppError> { /* JSON, then raw-password fallback */ }
    fn write(&self, id: &str, secrets: &ConnectionSecrets) -> Result<(), AppError> { /* delete empty, otherwise JSON */ }
}
```

Use serde defaults for all newly added profile/input fields. Set the default of `verify_server_cert` to true so old profiles remain secure. Validate certificate bodies only when supplied and accept certificate, PKCS#8, RSA PKCS#1 and EC private-key PEM markers.

- [ ] **Step 4: Update every in-memory `SecretStore` implementation**

Change each test mock from `HashMap<String, String>` to `HashMap<String, ConnectionSecrets>`, and make its `write` clone the complete struct. Ensure existing password-only tests still construct `ConnectionSecrets { password: Some(...), ..Default::default() }`.

- [ ] **Step 5: Run the focused Rust regression suite**

Run: `cargo test --manifest-path src-tauri/Cargo.toml domain::profile persistence`

Expected: PASS, with old profile JSON and old raw password fixtures still loading.

- [ ] **Step 6: Commit the model/storage change**

```bash
git add src-tauri/src/domain/profile.rs src-tauri/src/domain/mod.rs src-tauri/src/persistence src-tauri/tests/support/mod.rs src-tauri/tests/persistence.rs src-tauri/src/commands/connections.rs src-tauri/src/redis/connection_manager.rs
git commit -m "扩展连接 TLS 模型并迁移结构化钥匙串"
```

### Task 2: 集中实现 Standalone TLS client builder

**Files:**
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/Cargo.lock`
- Modify: `src-tauri/src/redis/connection_manager.rs`
- Modify: `src-tauri/src/commands/connections.rs`

**Interfaces:**
- Produces `fn build_client(profile: &ConnectionProfile, secrets: &ConnectionSecrets) -> Result<redis::Client, AppError>`。
- Produces TLS-aware `connection_url` and `connection_url_with_database` while preserving existing percent encoding and IPv6 handling。
- Changes `RedisOperations::test_connection` to receive resolved `ConnectionSecrets` rather than a password-only option。
- `open_connection`, `select_database`, Pub/Sub, Profiler and all ordinary operations continue to obtain clients through `RedisService::active` or the builder; no second `Client::open` path remains.

- [ ] **Step 1: Add failing TLS URL and builder tests**

```rust
#[test]
fn builds_rediss_url_when_tls_is_enabled() {
    let mut profile = valid_profile();
    profile.tls = true;
    assert_eq!(connection_url(&profile, Some("secret")).unwrap(),
        "rediss://:secret@127.0.0.1:6379/0");
}

#[test]
fn appends_insecure_marker_only_when_server_verification_is_disabled() {
    let mut profile = valid_profile();
    profile.tls = true;
    profile.verify_server_cert = false;
    assert!(connection_url(&profile, None).unwrap().ends_with("/0#insecure"));
}

#[test]
fn builds_tls_client_with_custom_ca_and_mtls_material_without_network_io() {
    let mut profile = valid_profile();
    profile.tls = true;
    let secrets = ConnectionSecrets {
        ca_certificate: Some(TEST_CERTIFICATE_PEM.into()),
        client_certificate: Some(TEST_CERTIFICATE_PEM.into()),
        client_key: Some(TEST_PRIVATE_KEY_PEM.into()),
        ..Default::default()
    };
    assert!(build_client(&profile, &secrets).is_ok());
}
```

- [ ] **Step 2: Run the focused tests and confirm RED**

Run: `cargo test --manifest-path src-tauri/Cargo.toml builds_rediss_url_when_tls_is_enabled appends_insecure_marker_only_when_server_verification_is_disabled builds_tls_client_with_custom_ca_and_mtls_material_without_network_io`

Expected: FAIL because the URL still uses `redis://` and no Rustls client builder exists.

- [ ] **Step 3: Enable the redis Rustls features and implement the builder**

Set the direct dependency features to include `tokio-rustls-comp`, `tls-rustls-insecure` and existing `tokio-comp`; add `uuid` with v4 support for the later import task if Cargo does not already expose it directly. Implement:

```rust
fn build_client(profile: &ConnectionProfile, secrets: &ConnectionSecrets) -> Result<Client, AppError> {
    let url = connection_url(profile, secrets.password.as_deref())?;
    if !profile.tls {
        return Client::open(url).map_err(|_| AppError::InvalidConnection);
    }
    let certificates = TlsCertificates {
        root_cert: secrets.ca_certificate.as_ref().map(|pem| pem.as_bytes().to_vec()),
        client_tls: match (&secrets.client_certificate, &secrets.client_key) {
            (Some(cert), Some(key)) => Some(ClientTlsConfig {
                client_cert: cert.as_bytes().to_vec(),
                client_key: key.as_bytes().to_vec(),
            }),
            (None, None) => None,
            _ => return Err(AppError::InvalidInput),
        },
    };
    Client::build_with_tls(url, certificates).map_err(|_| AppError::InvalidConnection)
}
```

Use `rediss://` for TLS and append `#insecure` only when `verify_server_cert` is false. Keep `profile.host` as the Rustls SNI name. Do not log the URL or PEM values.

- [ ] **Step 4: Route all connection entry points through the builder**

Replace the `Client::open(connection_url(...))` calls in `test_connection`, `open_connection` and `select_database`; read the complete `ConnectionSecrets` for open/select and pass command-resolved secrets for test. Keep the existing active-client replacement and Pub/Sub/Profiler cancellation order.

- [ ] **Step 5: Run service and URL tests**

Run: `cargo test --manifest-path src-tauri/Cargo.toml redis::connection_manager`

Expected: PASS, including existing credential encoding/TTL/observability tests and new TLS tests. If a cached redis feature is unavailable, stop at the dependency error and record the exact compiler output before changing the dependency strategy.

- [ ] **Step 6: Commit the TLS client change**

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/redis/connection_manager.rs src-tauri/src/commands/connections.rs src-tauri/src/redis/observability.rs
git commit -m "接入 Standalone Rustls TLS 连接"
```

### Task 3: 建立导入导出领域协议与归一化器

**Files:**
- Create: `src-tauri/src/domain/connection_transfer.rs`
- Modify: `src-tauri/src/domain/mod.rs`
- Test: `src-tauri/src/domain/connection_transfer.rs` unit tests

**Interfaces:**
- Produces `ConnectionExportDocument { version: u32, connections: Vec<ConnectionExportProfile> }`。
- Produces `ImportConnectionsInput { content: String }`。
- Produces `ImportConnectionsResult { imported, failed, ignored_secret_fields }` and `ConnectionImportFailure { index, name, code, message }`。
- Produces `normalize_import_document(&str) -> Result<NormalizedImportDocument, AppError>`，其中每条记录包含已归一化 Standalone 字段、证书名称提示、敏感字段计数和原始索引。

- [ ] **Step 1: Write failing normalization tests**

```rust
#[test]
fn export_document_contains_only_portable_fields() {
    let profile = profile_with_tls_and_secret_flags();
    let document = ConnectionExportDocument::from_profiles(&[profile]);
    let value = serde_json::to_value(document).unwrap();
    let raw = value.to_string();
    assert!(!raw.contains("password"));
    assert!(!raw.contains("BEGIN CERTIFICATE"));
    assert!(!raw.contains("BEGIN PRIVATE KEY"));
    assert!(value["connections"][0]["tls"].as_bool().unwrap());
}

#[test]
fn normalizes_redisinsight_aliases_and_counts_ignored_sensitive_fields() {
    let raw = serde_json::json!([{
        "connectionName": "TLS Redis", "host": "redis.example", "port": "6380",
        "db": "2", "authUser": "default", "password": "secret",
        "ssl": true, "verifyServerCert": false,
        "caCert": {"name": "Root CA", "value": "-----BEGIN CERTIFICATE-----secret"}
    }]).to_string();
    let normalized = normalize_import_document(&raw).unwrap();
    assert_eq!(normalized.entries[0].name, "TLS Redis");
    assert_eq!(normalized.entries[0].database, 2);
    assert!(normalized.entries[0].tls);
    assert!(!normalized.entries[0].verify_server_cert);
    assert_eq!(normalized.ignored_secret_fields, 2);
}

#[test]
fn rejects_non_standalone_connection_types_without_downgrading_them() {
    let raw = serde_json::json!({"connections": [{
        "name": "cluster", "host": "redis.example", "port": 6379,
        "connectionType": "CLUSTER", "nodes": []
    }]}).to_string();
    let normalized = normalize_import_document(&raw).unwrap();
    assert_eq!(normalized.entries[0].unsupported_type.as_deref(), Some("CLUSTER"));
}
```

- [ ] **Step 2: Run the normalization tests and confirm RED**

Run: `cargo test --manifest-path src-tauri/Cargo.toml connection_transfer`

Expected: FAIL because the transfer DTOs and alias parser do not exist.

- [ ] **Step 3: Implement versioned export DTOs and safe import normalization**

Serialize exactly the v1 fields from the spec. For import, accept a top-level array, `connections` or `profiles`; map `name/connectionName`, `database/db`, `username/authUser`, `tls/ssl` and `verify_server_cert/verifyServerCert`. Accept booleans and numeric strings. Reject missing name/host, invalid port/database and non-Standalone `connectionType`/topology markers as per-entry failures. Count but never retain password, auth, PEM bodies, client keys or SSH secrets. Preserve only non-sensitive certificate names.

- [ ] **Step 4: Run domain and serialization tests**

Run: `cargo test --manifest-path src-tauri/Cargo.toml connection_transfer domain::profile`

Expected: PASS, with export serialization assertions proving no sensitive field can appear.

- [ ] **Step 5: Commit the transfer protocol**

```bash
git add src-tauri/src/domain/connection_transfer.rs src-tauri/src/domain/mod.rs
git commit -m "增加连接导入导出领域协议"
```

### Task 4: 接入连接导入导出 commands 与批量持久化

**Files:**
- Modify: `src-tauri/src/commands/connections.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/tests/commands.rs`
- Modify: `src-tauri/tests/support/mod.rs`

**Interfaces:**
- Adds `export_connections(state) -> Result<ConnectionExportDocument, AppError>`。
- Adds `import_connections(state, input: ImportConnectionsInput) -> Result<ImportConnectionsResult, AppError>`。
- Keeps valid imports append-only with `Uuid::new_v4()` IDs and never calls `SecretStore::write` for imported entries.

- [ ] **Step 1: Write failing command tests**

Add tests around `export_connections_inner`/`import_connections_inner` using an in-memory profile repository and structured secret store:

```rust
#[tokio::test]
async fn export_connections_omits_password_and_certificate_material() {
    let state = test_state(vec![profile_with_tls_and_secret_flags()], secret_store_with_all_material());
    let document = export_connections_inner(&state).await.unwrap();
    let raw = serde_json::to_string(&document).unwrap();
    assert!(!raw.contains("password"));
    assert!(!raw.contains("BEGIN CERTIFICATE"));
}

#[tokio::test]
async fn import_appends_valid_entries_with_fresh_ids_and_reports_invalid_entries() {
    let state = test_state(vec![existing_profile()], empty_secret_store());
    let result = import_connections_inner(&state, ImportConnectionsInput {
        content: serde_json::json!({"connections": [
            {"id": "existing", "name": "Imported", "host": "127.0.0.1", "port": 6379},
            {"name": "Broken", "host": "", "port": 0}
        ]}).to_string(),
    }).await.unwrap();
    assert_eq!(result.imported.len(), 1);
    assert_ne!(result.imported[0].id, "existing");
    assert_eq!(result.failed.len(), 1);
    assert_eq!(state.profiles.load().unwrap().len(), 2);
}
```

- [ ] **Step 2: Run command tests and confirm RED**

Run: `cargo test --manifest-path src-tauri/Cargo.toml commands::connections`

Expected: FAIL because the commands and batch helpers are not registered.

- [ ] **Step 3: Implement export and import transaction flow**

For export, load profiles and call `ConnectionExportDocument::from_profiles`; never read secrets. For import, normalize first, convert valid entries to new `ConnectionProfile` values with `tls`/verification/name hints and all actual `has_*` flags false, append to the loaded list, and call `ProfileRepository::save` once. Return all per-entry failures and ignored secret count. If save fails, return `PERSISTENCE_FAILED` and leave the repository unchanged.

- [ ] **Step 4: Register commands and assert Tauri adapters**

Add both functions to `commands::connections` re-exports and `tauri::generate_handler!`, then add `let _ = connections::export_connections;` and `let _ = connections::import_connections;` to `src-tauri/tests/commands.rs`.

- [ ] **Step 5: Run Rust command and IPC tests**

Run: `cargo test --manifest-path src-tauri/Cargo.toml commands`

Expected: PASS, including snake_case argument decoding and no-secret import/export behavior.

- [ ] **Step 6: Commit the command integration**

```bash
git add src-tauri/src/commands/connections.rs src-tauri/src/commands/mod.rs src-tauri/src/lib.rs src-tauri/tests/commands.rs src-tauri/tests/support/mod.rs
git commit -m "接入连接导入导出 Tauri 命令"
```

### Task 5: 扩展 typed bridge 与 TLS 连接表单

**Files:**
- Modify: `src/lib/types.ts`
- Modify: `src/lib/tauri.ts`
- Modify: `src/features/connections/connectionState.ts`
- Modify: `src/features/connections/ConnectionForm.tsx`
- Modify: `src/features/connections/connections.test.tsx`

**Interfaces:**
- Adds TypeScript `ConnectionSecretsFormInput`, `ConnectionExportDocument`, `ImportConnectionsInput`, `ImportConnectionsResult` and `ConnectionImportFailure` matching Rust snake_case DTOs.
- `SaveConnectionInput` contains nullable `ca_certificate`, `client_certificate`, `client_key` and boolean clear flags; old edit behavior uses null to preserve.
- Adds `exportConnections()` and `importConnections(input)` wrappers in `src/lib/tauri.ts`.

- [ ] **Step 1: Add failing frontend tests for TLS form behavior**

Extend the existing page tests with assertions like:

```ts
it("保存 TLS 表单时发送校验开关和证书材料", async () => {
  render(<ConnectionPage onOpenConnection={onOpenConnectionMock} />);
  await openNewConnectionForm();
  fillStandaloneForm();
  fireEvent.click(screen.getByLabelText("启用 TLS"));
  fireEvent.change(screen.getByLabelText("CA 证书"), {
    target: { value: TEST_CERTIFICATE_PEM },
  });
  fireEvent.change(screen.getByLabelText("CA 名称"), {
    target: { value: "Root CA" },
  });
  fireEvent.click(screen.getByRole("button", { name: "保存" }));
  await waitFor(() => expect(saveConnectionMock).toHaveBeenCalled());
  expect(saveConnectionMock.mock.calls[0][0]).toMatchObject({
    profile: expect.objectContaining({ tls: true, verify_server_cert: true }),
    ca_certificate: TEST_CERTIFICATE_PEM,
    clear_ca_certificate: false,
  });
});
```

Also add a test that editing a profile with existing certificate flags and leaving PEM inputs blank sends `null` (preserve), while selecting clear sends `clear_ca_certificate: true`.

- [ ] **Step 2: Run the focused frontend tests and confirm RED**

Run: `npm test -- src/features/connections/connections.test.tsx`

Expected: FAIL because TLS controls, fields and bridge types do not exist.

- [ ] **Step 3: Implement typed models and form state conversion**

Update `ConnectionProfile` and all test fixtures with the six new fields. Make `formValuesFromProfile` default to `tls=false` and `verify_server_cert=true`; keep certificate body fields empty on edit. Build the profile with TLS metadata and names, and build save/test inputs with null for blank secrets, preserving old flags when no new body is supplied. Use explicit clear checkboxes/buttons.

- [ ] **Step 4: Implement the TLS form section and client-side hints**

Render labeled controls:

```tsx
<input type="checkbox" aria-label="启用 TLS" />
<input type="checkbox" aria-label="验证服务端证书" />
<input aria-label="CA 名称" />
<textarea aria-label="CA 证书" />
<input aria-label="客户端证书名称" />
<textarea aria-label="客户端证书" />
<textarea aria-label="客户端私钥" />
```

Show a local format warning when a non-empty body lacks the corresponding PEM marker; keep Rust validation authoritative. Explain that host is used as SNI and certificate bodies are never returned when editing.

- [ ] **Step 5: Run frontend tests and build type-check**

Run: `npm test -- src/features/connections/connections.test.tsx && npm run build`

Expected: PASS, including all existing connection tests updated for the expanded typed input.

- [ ] **Step 6: Commit typed bridge and form**

```bash
git add src/lib/types.ts src/lib/tauri.ts src/features/connections/connectionState.ts src/features/connections/ConnectionForm.tsx src/features/connections/connections.test.tsx
git commit -m "完善连接 TLS 证书表单与 typed bridge"
```

### Task 6: 增加连接页导入导出与状态反馈

**Files:**
- Modify: `src/features/connections/ConnectionPage.tsx`
- Modify: `src/features/connections/ConnectionList.tsx`
- Modify: `src/features/connections/connections.test.tsx`
- Modify: `src/styles.css`

**Interfaces:**
- The page calls `importConnections({ content })` after reading a selected JSON file and updates the profile list with `result.imported`.
- The page calls `exportConnections()`, serializes the returned document and downloads `redix-connections.json` via a Blob URL.
- No file path is sent to Rust, and no certificate/password value is placed in the export document.

- [ ] **Step 1: Add failing page tests for file actions**

```ts
it("导出连接时下载 typed IPC 返回的无 secrets JSON", async () => {
  exportConnectionsMock.mockResolvedValue({ version: 1, connections: [] });
  render(<ConnectionPage onOpenConnection={onOpenConnectionMock} />);
  await screen.findByText("还没有 Redis 连接");
  fireEvent.click(screen.getByRole("button", { name: "导出连接" }));
  await waitFor(() => expect(exportConnectionsMock).toHaveBeenCalledTimes(1));
  expect(createdDownloadText).not.toContain("password");
  expect(createdDownloadText).not.toContain("BEGIN PRIVATE KEY");
});

it("导入文件限制 10 MB 并显示部分成功结果", async () => {
  importConnectionsMock.mockResolvedValue({
    imported: [localProfile],
    failed: [{ index: 1, name: "bad", code: "INVALID_CONNECTION", message: "连接配置无效" }],
    ignored_secret_fields: 1,
  });
  render(<ConnectionPage onOpenConnection={onOpenConnectionMock} />);
  const input = screen.getByLabelText("导入连接文件");
  fireEvent.change(input, { target: { files: [new File(['{"connections":[]}'], "connections.json", { type: "application/json" })] } });
  await waitFor(() => expect(importConnectionsMock).toHaveBeenCalled());
  expect(screen.getByRole("status")).toHaveTextContent("部分导入");
});
```

- [ ] **Step 2: Run the page tests and confirm RED**

Run: `npm test -- src/features/connections/connections.test.tsx`

Expected: FAIL because the transfer buttons, bridge mocks and file handlers do not exist.

- [ ] **Step 3: Implement import/export file handlers**

Add a visible import file input with `accept="application/json,.json"` and a 10 MiB check before `File.text()`. On success, merge imported profiles, keep the page mounted, and show counts/warnings. On partial result, list failed names/messages without hiding valid profiles. On IPC or file errors, map through `toUserFacingError`. For export, use:

```ts
const blob = new Blob([JSON.stringify(document, null, 2)], { type: "application/json" });
const url = URL.createObjectURL(blob);
const link = document.createElement("a");
link.href = url;
link.download = "redix-connections.json";
link.click();
URL.revokeObjectURL(url);
```

Keep the serialized document limited to typed export DTO fields.

- [ ] **Step 4: Add TLS/certificate status to cards and responsive styles**

Show `TLS 已启用`/`TLS 未启用`, `证书已配置` and `证书需重新录入` based on profile flags and non-sensitive names. Add only the CSS needed for the TLS form, transfer actions and narrow screens; do not change the existing workspace layout or create Cloud/SQL navigation.

- [ ] **Step 5: Run focused frontend tests and type/build checks**

Run: `npm test -- src/features/connections/connections.test.tsx && npm run build`

Expected: PASS, including existing open/edit/delete/switch behavior and new transfer feedback.

- [ ] **Step 6: Commit the connection-page transfer UI**

```bash
git add src/features/connections/ConnectionPage.tsx src/features/connections/ConnectionList.tsx src/features/connections/connections.test.tsx src/styles.css
git commit -m "增加连接配置导入导出界面"
```

### Task 7: 全量验证、TLS 集成入口与交付记录

**Files:**
- Modify: `src-tauri/tests/redis_integration.rs` only if the existing ignored integration harness can host a TLS-specific environment-gated test without leaking credentials
- Modify: `task_plan.md`
- Modify: `findings.md`
- Modify: `progress.md`
- Create: `.superpowers/sdd/2026-08-24-connection-import-export-standalone-tls/final-report.md`

**Interfaces:**
- No production API changes in this task. It records verification evidence and leaves real TLS integration explicitly ignored when `REDIX_TEST_REDIS_TLS_URL` and certificate environment variables are absent.

- [ ] **Step 1: Run the Rust regression matrix**

Run: `cargo test --manifest-path src-tauri/Cargo.toml`.

Expected: all ordinary library, command, domain and persistence tests pass; any external Redis/TLS tests remain ignored unless their required environment is present.

- [ ] **Step 2: Run the frontend and product-scope matrix**

Run: `npm run test:frontend`, `npm run build`, and `npm run check:non-cloud`.

Expected: all frontend tests pass, TypeScript/Vite production build passes, and the non-Cloud scope check passes.

- [ ] **Step 3: Run formatting and secret-safety checks**

Run: `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check` and `git diff --check`. Also inspect the serialized export tests and run `rg -n "password|BEGIN (CERTIFICATE|PRIVATE KEY)" src-tauri/src/domain/connection_transfer.rs src/features/connections` to verify matches are only field names, labels, or negative assertions, never export construction.

- [ ] **Step 4: Record environment-gated integration status**

If `REDIX_TEST_REDIS_TLS_URL` and complete PEM variables are set, run the ignored TLS test and report the server/certificate result without printing secret values. If absent, record that real TLS was not claimed and keep the test ignored.

- [ ] **Step 5: Write the final verification report and update planning files**

Record exact command results, test counts, ignored-test reasons, dependency feature status, and any residual limitation (host-as-SNI, no certificate file paths) in `.superpowers/sdd/2026-08-24-connection-import-export-standalone-tls/final-report.md`. Mark Phase 13 checklist and progress status complete only after all required checks pass.

- [ ] **Step 6: Commit verification artifacts**

```bash
git add task_plan.md findings.md progress.md .superpowers/sdd/2026-08-24-connection-import-export-standalone-tls/final-report.md
git commit -m "完成连接导入导出与 Standalone TLS 验证"
```

## Self-review checklist

- [ ] Spec sections 1–3 are covered by Tasks 1, 2, 5 and 6; non-Cloud/SQL boundaries are global constraints and final scope validation.
- [ ] Spec section 4 is covered by Task 1, including legacy profile defaults, structured secrets, save semantics and PEM validation.
- [ ] Spec section 5 is covered by Task 2, including `rediss://`, custom CA, mTLS, insecure marker, host-as-SNI and all connection paths.
- [ ] Spec section 6 is covered by Tasks 3, 4 and 6, including v1 DTO, RedisInsight aliases, append-only fresh IDs, partial results, 10 MiB file limit and Blob download.
- [ ] Spec section 7 is covered by Tasks 4 and 5 with typed snake_case IPC and fixed error serialization.
- [ ] Spec section 8 is covered by every task's focused tests and Task 7's full matrix.
- [ ] Type names and field names are consistent across Rust and TypeScript: `verify_server_cert`, `ca_certificate_name`, `client_certificate_name`, `has_ca_certificate`, `has_client_certificate`, `ignored_secret_fields`.
- [ ] No step relies on a placeholder or asks an implementer to infer a missing function signature.
