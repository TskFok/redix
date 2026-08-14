# Task 4 报告：Rust Redis 服务和连接生命周期

## 状态

`DONE_WITH_ENVIRONMENT_LIMITATION`：Standalone Redis 服务、连接生命周期、SCAN、五类数据结构 CRUD、TTL 和 Workbench 命令执行已实现并通过离线编译/单元验证；本机 Redis socket 受环境限制，未执行真实 Redis 集成流。

## 实现范围

- `src-tauri/src/redis/connection_manager.rs`：`RedisService`、`RedisOperations`、Standalone URL、密码/连接错误映射、以 PING 为成功边界的 active client 生命周期（INFO server 为 best-effort，失败时版本为 `unknown`）、SCAN、五类型读写删除、TTL 和命令结果转换；Redis Map 作为 pair array 返回且 `kind` 为 `array`。
- `src-tauri/src/redis/key_ops.rs`：String、Hash、List、Set、Sorted Set 的纯类型转换和未知类型错误。
- `src-tauri/src/redis/workbench.rs`：空白、单双引号和反斜杠分词；未闭合引号/空命令返回固定 `COMMAND_FAILED`。
- `src-tauri/tests/redis_integration.rs`：基于 `REDIX_TEST_REDIS_URL` 的可选 Standalone 集成测试，使用测试内存 profile/secret double，不触碰真实钥匙串。实际流逐一验证五种类型的 set/get 返回、SCAN 分页元数据、TTL 读取、删除后的固定错误和 Workbench PING；无论流成功或失败，都会删除唯一前缀下的已知 key 并关闭连接。
- `src-tauri/Cargo.toml`、`Cargo.lock`、`src-tauri/src/lib.rs`：Redis/Tokio 依赖和模块导出。

## TDD 和验证

先有 tokenizer、类型转换和 URL/TTL 失败测试骨架；实现过程中 Cargo 首次编译暴露了 Redis 1.5 的 `Value::BigNumber` 转换和异步 query 类型推断问题，随后使用本地 crate API 修正。最终命令结果：

```text
CARGO_NET_OFFLINE=true cargo test --manifest-path src-tauri/Cargo.toml
10 个 Redis/服务单元测试、6 个 domain 测试、9 个 persistence 测试通过；redis_integration 为 1 个 ignored；0 个失败。

CARGO_NET_OFFLINE=true cargo test --manifest-path src-tauri/Cargo.toml --test redis_integration -- --nocapture
0 passed; 1 ignored（未设置 REDIX_TEST_REDIS_URL）

REDIX_TEST_REDIS_URL=redis://127.0.0.1:6379 CARGO_NET_OFFLINE=true cargo test --manifest-path src-tauri/Cargo.toml --test redis_integration -- --ignored --nocapture
未执行：当前环境未设置 REDIX_TEST_REDIS_URL，且本机 Redis TCP socket 不可访问；不能将 ignored 视为真实集成通过。

cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
通过

git diff --check
通过
```

完整离线测试仍会输出 `tests/support/mod.rs` 中既有的 `invalid_profile` dead-code warning；本次未修改该非 Task 4 审查范围内的测试辅助代码。

本机 `REDIX_TEST_REDIS_URL` 未设置，且 `nc -vz 127.0.0.1 6379` 返回 `Operation not permitted`，因此没有伪造 Redis 集成通过。集成测试保留为显式 ignored：普通命令必须显示 ignored；只有设置环境变量并追加 `--ignored --nocapture` 才会运行真实 PING、五种类型读写、SCAN 分页、TTL、删除和 Workbench PING。

## 范围审查

- Browser 使用 `SCAN cursor MATCH pattern COUNT count`；源码/测试范围扫描未命中阻塞式全量扫描命令。
- 未发现 Redis Cloud/Azure/cloud SDK、云登录/发现、SQL 或循环 SQL 查询。
- 生产错误映射为固定 `AppError`，不把 Redis URI、密码或底层错误文本回传。

## Concerns

1. 当前环境没有可用的本地 Redis socket；真实五类型集成流需要在开发机设置 `REDIX_TEST_REDIS_URL` 后运行 `CARGO_NET_OFFLINE=true cargo test --manifest-path src-tauri/Cargo.toml --test redis_integration -- --ignored --nocapture`。
2. 现有 `tests/support/mod.rs` 的 `invalid_profile` 在 persistence test 中有既有 dead-code warning，不影响测试结果。
