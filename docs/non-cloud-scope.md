# Redix 非 Cloud 范围

本文档是人工审查边界，不属于生产代码扫描输入。生产检查只读取 `src/`、`src-tauri/src/` 和 `package.json`，因此本文件中的排除项说明不会被误判为产品入口。

## 允许项

- 本地 Redis Standalone TCP 连接和连接配置管理。
- 使用系统钥匙串保存本地连接密码；前端 DTO 和连接列表不暴露密码。
- Browser 使用 `SCAN`、`MATCH`、`COUNT` 分页列出键，并读取键类型、TTL 和值。
- String、Hash、List、Set、Sorted Set 的基础读取、编辑、删除和 TTL 操作。
- Workbench 在已打开的本地连接上执行 Redis 命令，并展示结构化结果。
- React/Tauri 本地 UI、前端测试、Rust 单元测试和本地构建工具链。

## 明确排除项

- Redis Cloud、Azure Managed Redis 及其他云托管 Redis 产品。
- 云登录、云账户、云 SDK、云 API、云端点和云数据库发现。
- Cluster、Sentinel、TLS、SSH、远程托管实例和云资源管理。
- Redis 模块功能，包括模块专用数据类型、模块查询和模块可视化。
- Profiler、Slow Log、Pub/Sub 等非 MVP 运营功能。

## 人工审查清单

- [x] 生产扫描目录固定为 `src/`、`src-tauri/src/`、`package.json`，不包含文档范围说明。
- [x] 入口词扫描大小写不敏感，命中后打印文件和词并以非零状态退出。
- [x] Browser 代码路径使用 `SCAN` 分页，没有加入 `KEYS` 命令。
- [x] 未加入云 SDK、云端点、云登录、云账户模型或云凭据存储。
- [x] 未引入 SQL，也没有在循环中查询 SQL。
- [x] 现有前端和 Rust 测试仍需通过；真实 Redis、Tauri bundle 结果按实际环境记录。

## 测试命令

```bash
npm run check:non-cloud
npm run test:frontend
npm run build
npm run test:rust
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
CARGO_NET_OFFLINE=true cargo test --manifest-path src-tauri/Cargo.toml
npm run tauri:build
git diff --check
```

带真实 Redis 的集成测试需要显式设置 `REDIX_TEST_REDIS_URL`，并使用测试文件要求的 ignored 参数；未启动 Redis 时不能把 ignored 或未执行写成通过。
