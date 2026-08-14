# Findings & Decisions

## Requirements

- 参考 `/Users/ushopal/workspace/myself/RedisInsight` 的功能。
- 生成 Rust + Tauri 版本。
- 排除 Redis Cloud 相关功能。
- 默认在当前目录、当前分支工作；不擅自创建分支。
- 所有对用户的回复使用简体中文。
- 禁止在循环遍历中查询 SQL；本项目默认不引入 SQL 查询循环。

## Research Findings

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
