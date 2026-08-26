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

- 短 SHA：`2273751`
- 提交信息：`补充 RedisJSON 模块与路径领域协议`
