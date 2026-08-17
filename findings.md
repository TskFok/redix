# Findings & Decisions

## Requirements

- 参考 `/Users/ushopal/workspace/myself/RedisInsight` 的功能。
- 生成 Rust + Tauri 版本。
- 排除 Redis Cloud 相关功能。
- 默认在当前目录、当前分支工作；不擅自创建分支。
- 所有对用户的回复使用简体中文。
- 禁止在循环遍历中查询 SQL；本项目默认不引入 SQL 查询循环。

## Research Findings

- 2026-08-14 Task 4：用户已明确批准现有设计和实现计划，要求在当前 `main` 分支直接实现 Redis 核心连接服务并提交，不修改 Task 1-3、commands 或前端。
- Task 4 已完成 Redis Standalone 生命周期、SCAN、五类 CRUD、TTL、Workbench 命令执行和可选集成测试；真实 Redis socket 在当前沙箱受限，集成流按要求保留为显式 ignored。
- Task 4 明确接口为 `RedisOperations` 九个异步方法；active map 仅保存 `redis::Client`，每次业务操作重新获取 multiplexed async connection；Browser 只能发 `SCAN cursor MATCH pattern COUNT count`。
- `key_ops.rs`、`workbench.rs` 和 `connection_manager.rs` 的生产错误边界使用固定 `AppError`，不回传 URI、密码或 Redis 底层错误文本。
- `AppError` 是无底层文本载荷的固定枚举，天然适合作为 Redis 错误安全映射边界。

- `/Users/ushopal/workspace/myself/redix` 初始为空目录，不存在 Tauri、Rust、前端或 Git 骨架。
- RedisInsight 当前是 Electron + React 18 + TypeScript + Redux Toolkit + NestJS 的桌面应用。
- 参考项目 README 列出的主要非 Cloud 能力包括：
  - Redis 数据库连接与管理。
  - 键浏览、过滤、可视化以及多种值格式查看。
  - string、list、hash、set、sorted set、stream、JSON、vector set、array 的 CRUD（后几项依赖 Redis 模块/版本）。
  - Workbench、命令自动补全、原始模式与结果可视化。
  - Profiler、Slow Log、Pub/Sub、批量删除。
  - Search/Query 索引和结果可视化、模块能力、可扩展可视化插件。
- 参考 README 同时包含 Redis Cloud 支持、Azure Managed Redis 支持、Telemetry 等内容；这些需要在新版本中显式分层/排除，避免将 Cloud 登录、云账户、云 API、云数据库发现等逻辑带入。
- 参考项目的 `redisinsight/api/src` 中存在 `cloud`、`azure` 等模块，不能直接作为新版本的默认后端边界。
- 本机已安装 Rust 1.93.1、Cargo 1.93.1、Node 25.5.0、npm 11.17.0、Git 2.52.0 和 Redis Server 8.4.0，可直接进行 Tauri/Rust 开发与本地 Redis 验证。
- 参考项目 UI 已将 Browser、Workbench、Connection 等能力拆成独立页面/组件；新版本可沿用“左侧连接/导航 + 中央数据工作区 + 右侧详情/命令结果”的产品心智模型，但不复制 Electron/NestJS 代码。
- 本机已有 Tauri React 工程使用 Tauri API/CLI 2.x、Vite、Vitest、Testing Library、React 19 和 `keyring`，可作为工程配置参考；本项目仍只选用 MVP 所需依赖。
- `cargo search` 返回 Redis Rust crate `redis 1.5.0`、Keyring crate `keyring 4.1.6`，npm registry 返回 `@tauri-apps/cli 2.11.4`、`@tauri-apps/api 2.11.1`。

## Technical Decisions

| Decision | Rationale |
|----------|-----------|
| 待用户确认 MVP 后选择架构 | 全量非 Cloud 重构范围过大，需要先确定首个可交付闭环 |
| 新项目优先由 Tauri Rust 命令处理 Redis 连接与数据访问 | 可避免 Electron/NestJS 双进程与 Node 运行时依赖，桌面端数据访问边界更清晰 |
| Cloud 功能不作为 feature flag 暴露 | 用户要求排除，而不是默认关闭；代码、菜单、依赖和网络端点都不应包含 Cloud 能力 |
| 首个 MVP 聚焦 Standalone + Browser + Workbench | 先形成可运行的端到端闭环，减少一次性移植大型产品的风险 |

## Architecture Options

1. Tauri 2 + React/Vite + Rust Redis domain：前端适合快速实现接近 RedisInsight 的多面板交互，Rust 通过 Tauri commands 暴露连接、扫描、读写和命令执行；依赖适中，推荐用于本项目。
2. Tauri 2 + 原生 HTML/CSS/TypeScript + Rust：依赖最少、启动轻，但复杂的表格、编辑器和状态管理需要手工维护，后续扩展成本较高。
3. Tauri 2 + React/Vite + 本地 Axum HTTP 服务 + Rust Redis domain：接口边界清晰但会引入额外本地服务、端口/生命周期和安全复杂度；对当前 MVP 属于过度设计。

## Proposed Design Direction

- 选择方案 1：Tauri 2 负责桌面壳和 IPC，Rust 负责 Redis 连接/命令/本地存储，React/Vite 负责 UI。
- 首版只支持 Redis Standalone TCP 连接（host、port、用户名、密码、TLS 先不出现在首版表单）；后续可在不改前端领域模型的情况下加入安全连接和拓扑类型。
- 前端页面保持三个核心工作区：连接页、Browser、Workbench；默认主题采用深色 Redis 风格，但首版同时保留系统浅色主题可选项。
- Redis Cloud 排除为架构约束：不引入云 SDK、不保存云凭据、不展示云菜单、不调用云端点、不提供云发现/登录。

## Issues Encountered

## Task 10 初始核对（2026-08-17）

- 工作区位于 `main` 分支且初始干净；本任务不创建分支。
- `README.md` 当前不存在，`package-lock.json` 存在且需要随 `package.json` scripts 变更保持一致。
- `src-tauri/icons/icon.png`、`icon.ico`、`icon.icns`、`icon.svg` 及平台资源均已存在，因此只需引用既有资源，不下载外部图标。
- `src-tauri/tauri.conf.json` 当前 identifier 为 `com.redix.app`，bundle `active` 为 `false`；Task 10 需改为正式配置。
- `src/styles.css` 已有深色基础、44px 部分控件和 reduced-motion，但尚无浅色主题类/系统偏好、统一 `role="alert"`/`role="status"` 样式与明确的 375/768/1024 响应式分层。
- RED 阶段测试已按要求先写；运行 `npm test -- --run scripts/check-non-cloud-scope.test.mjs` 因实现文件缺失而退出 1，失败原因符合预期。
- GREEN 阶段同一命令通过 2/2；扫描器按文本位置排序、大小写不敏感并去重，生产入口不读取文档和脚本目录。
- `npm install --package-lock-only --ignore-scripts --offline` 报告 lockfile 与依赖已同步，未产生 package-lock 差异。
- `npm run check:non-cloud` 通过；`npm run test:frontend` 通过 6 个测试文件、55 个测试；`npm run build` 通过。
- `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check` 通过；`CARGO_NET_OFFLINE=true cargo test --manifest-path src-tauri/Cargo.toml` 通过既有 Rust 测试（23 + 1 + 6 + 9），Redis 集成测试 1 项按设计 ignored，另有既有 `tests/support/mod.rs::invalid_profile` dead-code 警告。
- `npm run tauri:build` 的 Rust release 编译和 `.app` 生成成功，但整体退出码为 1。DMG 阶段的 `bundle_dmg.sh` 失败；直接诊断显示 `hdiutil resize: failed. 设备未配置 (6)`，属于当前桌面沙箱/镜像设备环境限制，未伪造为通过。
- 在沙箱外重跑 `npm run tauri:build` 成功，最终生成 `src-tauri/target/release/bundle/macos/Redix.app` 和 `src-tauri/target/release/bundle/dmg/Redix_0.1.0_aarch64.dmg`。
- 沙箱外本机 Redis 返回 `PONG`，真实 Standalone 集成测试通过 1/1；前端 55/55、Rust 39 个普通测试、非 Cloud 扫描、格式检查和生产构建均通过。

## Task 9 独立复审观察（2026-08-17）

- 当前提交为 `5bcfe489dcaf578c69e9d9c643678d633f30fbfc`，工作区初始干净。
- 当前 Workbench 已将 `connectionId?.trim()` 用于守卫、展示和 IPC payload；命令使用 `trim()` 后的值写入状态、IPC 和成功历史。
- 当前 Workbench 错误映射按固定 code 白名单生成固定文案，未知 code 降级为 `COMMAND_FAILED`；原始 `message` 不进入 Workbench alert。
- 当前连接切换顺序为 `open(B)` → `close(A)` → 通知父级切换成功；关闭 A 失败时尝试 `close(B)` 并恢复前端 activeId 为 A。Task 9 修复复审已确认该异常路径有回归测试且不会把新连接泄漏给前端。

| Issue | Resolution |
|-------|------------|
| 目标目录没有既有工程可增量修改 | 先完成范围确认和设计，再初始化 Tauri 工程 |
| 规划文件第一次更新补丁未命中上下文 | 重新读取文件并改用精确补丁 |
| `cargo info` 无法解析 crates.io | 记录错误；使用已成功返回的 `cargo search` 版本信息制定计划，依赖安装阶段再申请网络权限 |
| Tauri `generate_context!` 在 Rust 编译阶段要求 `src-tauri/icons/icon.png` | 用户已确认将最小图标资源前移到 Task 1；Task 10 仅保留正式 bundle 配置，避免领域测试被后置资源阻塞 |

## Resources

- 参考项目 README：`/Users/ushopal/workspace/myself/RedisInsight/README.md`
- 参考项目约束：`/Users/ushopal/workspace/myself/RedisInsight/AGENTS.md`
- 参考项目提交：`48ee19fab`（当前 `main`）

## Visual/Browser Findings

- 尚未进行视觉/浏览器检查；当前阶段通过本地源码和文档完成范围摸底。
