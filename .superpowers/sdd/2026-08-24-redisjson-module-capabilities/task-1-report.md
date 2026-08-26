# Task 1 报告

## 实现内容

- 新增 `src-tauri/src/domain/module_capabilities.rs`
  - 定义 `GetModuleCapabilitiesInput`
  - 定义 `ModuleCapabilities`
  - 实现 `GetModuleCapabilitiesInput::validate()`
  - 实现 `ModuleCapabilities::from_modules()`，按大小写不敏感识别 `ReJSON` / `RedisJSON`，保留首个非空版本字符串
- 新增 `src-tauri/src/domain/json_path.rs`
  - 定义 `GetJsonPathInput`
  - 定义 `SetJsonPathInput`
  - 定义 `AppendJsonArrayInput`
  - 定义 `DeleteJsonPathInput`
  - 定义 `JsonPathValue`
  - 定义 `JsonMutationResult`
  - 实现 `validate_json_path()`
  - 实现 `normalize_json_path()`
  - 实现 `validate_json_array_append()`
  - 实现各输入 DTO 的 `validate()`
- 修改 `src-tauri/src/domain/mod.rs`
  - 注册并导出 `json_path` 与 `module_capabilities`
- 修改 `src-tauri/tests/domain.rs`
  - 补充 JSON path / payload / legacy path 合同测试

## 修改文件

- `src-tauri/src/domain/module_capabilities.rs`
- `src-tauri/src/domain/json_path.rs`
- `src-tauri/src/domain/mod.rs`
- `src-tauri/tests/domain.rs`
- `.superpowers/sdd/2026-08-24-redisjson-module-capabilities/task-1-report.md`

## TDD 记录

### RED

命令：

```bash
cargo test --manifest-path src-tauri/Cargo.toml --test domain json_path -- --nocapture
```

关键输出：

```text
error[E0432]: unresolved imports `redix_lib::domain::normalize_json_path`, `redix_lib::domain::validate_json_array_append`, `redix_lib::domain::validate_json_path`, `redix_lib::domain::GetJsonPathInput`, `redix_lib::domain::SetJsonPathInput`
error: could not compile `redix` (test "domain") due to 1 previous error
```

结论：

- RED 成立，失败原因是 JSON path DTO 与校验合同尚未实现，符合任务简报预期

### GREEN

命令：

```bash
cargo test --manifest-path src-tauri/Cargo.toml --test domain json_path -- --nocapture
```

关键输出：

```text
running 2 tests
test json_path_inputs_reject_empty_or_unsafe_paths ... ok
test json_path_supports_legacy_and_modern_root_forms ... ok
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 22 filtered out
```

结论：

- 聚焦命令通过，命中的 `json_path` 命名测试全部为绿

### Domain 回归

命令：

```bash
cargo test --manifest-path src-tauri/Cargo.toml --test domain
```

关键输出：

```text
running 24 tests
...
test json_payload_and_array_append_limits_are_enforced ... ok
test result: ok. 24 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

结论：

- 新增测试和现有 domain 回归全部通过

## 与 spec/brief 一致性

- 未发现 `task-1-brief.md` 与设计文档存在冲突
- 本次以简报为直接执行合同实现，未扩展到 Redis service、Tauri command、前端或 Redis Cloud 能力

## 自审结果

- DTO 字段保持 snake_case 命名
- 含 `serde_json::Value` 的 DTO 使用 `PartialEq`，未强求 `Eq`
- `connection_id` / `key` 采用 trim 后非空校验
- JSON path 按 UTF-8 字节长度限制为 `1..=512`
- 路径拒绝控制字符、`*`、`..`、`?`、`;`
- legacy path 仅在 `legacy=true` 时将 `$` 归一化为 `.`，并将 `$.field` 归一化为 `.field`
- JSON payload 按 `serde_json::to_vec` 限制为不超过 5 MiB
- 数组追加限制为 `1..=500` 项，并施加同样的 5 MiB 聚合负载限制
- 未修改现有 `RedisValue::Json` root document 合同
- 未触碰 SQL、KEYS、shell、secret 或 cloud 相关逻辑

## Concern

- 简报指定的聚焦命令使用 `cargo test ... json_path` 作为测试过滤条件，因此只匹配到了测试名包含 `json_path` 的 2 个新增测试；第三个新增测试 `json_payload_and_array_append_limits_are_enforced` 是通过完整 `domain` 回归验证通过的，而不是被该聚焦命令直接命中

## 提交信息

- 短 SHA：`9f3fe0b`
- 提交信息：`补充任务一报告提交信息`

## Review Fix Report

### 背景

- Review 指出 `src-tauri/src/domain/json_path.rs` 的 `validate_json_path()` 与 `task-1-brief.md` 不一致
- 具体问题是：domain 合同要求 path 只需以 `$` 或 `.` 开头，但当前实现会在 `allow_legacy_root=false` 时拒绝诸如 `.user` 这样的合法点路径
- 同时，Rust domain 测试尚未固定以下边界合同：
  - 512 UTF-8 字节路径上限
  - 500 项数组追加上限
  - `connection_id` / `key` trim 后非空
  - `ModuleCapabilities::from_modules()` 的大小写不敏感识别与首个非空版本选择

### 本次改动

- 修改 `src-tauri/src/domain/json_path.rs`
  - 修正 `validate_json_path()`，允许合法的点路径（如 `.user.name`）
  - 保持路径必须以 `$` 或 `.` 开头、控制字符和 `*` / `..` / `?` / `;` 拒绝、UTF-8 字节长度限制、legacy normalize 语义不变
- 修改 `src-tauri/tests/domain.rs`
  - 为 `.user.name` 添加合法路径覆盖
  - 固定 `connection_id` / `key` trim 后非空
  - 固定 512 UTF-8 字节路径上限
  - 固定 500 项数组追加上限与 501 项拒绝
  - 固定 `AppendJsonArrayInput` 边界行为
  - 固定 `ModuleCapabilities::from_modules()` 的大小写识别与首个非空版本选择

### TDD RED

命令：

```bash
cargo test --manifest-path src-tauri/Cargo.toml --test domain json_path -- --nocapture
```

关键输出：

```text
running 3 tests
thread 'json_path_inputs_reject_empty_or_unsafe_paths' ... assertion `left == right` failed
  left: Err(InvalidInput)
 right: Ok(())
test result: FAILED. 2 passed; 1 failed
```

结论：

- RED 成立，失败点准确落在 `.user.name` 被错误拒绝，和 review finding 一致

### 覆盖测试 GREEN

命令：

```bash
cargo test --manifest-path src-tauri/Cargo.toml --test domain json_path -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test domain module_capabilities_detects_json_modules_case_insensitively -- --nocapture
```

关键输出：

```text
running 3 tests
test json_path_supports_legacy_and_modern_root_forms ... ok
test json_path_contract_enforces_trimmed_identifiers_and_utf8_byte_limit ... ok
test json_path_inputs_reject_empty_or_unsafe_paths ... ok
test result: ok. 3 passed; 0 failed
```

```text
running 1 test
test module_capabilities_detects_json_modules_case_insensitively ... ok
test result: ok. 1 passed; 0 failed
```

### Domain 回归

命令：

```bash
cargo test --manifest-path src-tauri/Cargo.toml --test domain
```

关键输出：

```text
running 27 tests
test append_json_array_input_accepts_boundary_and_rejects_limit_overflow ... ok
test json_path_contract_enforces_trimmed_identifiers_and_utf8_byte_limit ... ok
test module_capabilities_detects_json_modules_case_insensitively ... ok
test json_payload_and_array_append_limits_are_enforced ... ok
test result: ok. 27 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

### 自审

- 修复后 `validate_json_path()` 与 brief 对齐：允许以 `.` 开头的合规路径
- legacy normalize 仍只在 `normalize_json_path(..., true)` 中把 `$` / `$.field` 转成 legacy 形式
- 生产改动仍限制在 Task 1 文件内，没有扩展到 Redis service、Tauri command、前端或 cloud 逻辑
