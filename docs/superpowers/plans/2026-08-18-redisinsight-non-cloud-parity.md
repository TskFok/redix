# RedisInsight 非 Cloud 功能差异补全实施总计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在不引入 Redis Cloud、Azure、AI、Telemetry 或远程插件的前提下，按确认的第一批顺序补齐 Redix 的 Browser 生产力、Workbench 增强、数据库/实例概览和 Query Library/设置能力。

**Architecture:** 延续 React + Vite + Tauri 2 + Rust。前端通过 typed Tauri wrapper 调用 Rust commands，Rust 通过 `RedisService` 访问当前 Standalone Redis，通过版本化 JSON 文件保存本地资源，通过系统钥匙串保存密码。Browser、Workbench、Database、Settings 分成独立 feature；长连接能力不进入本计划。

**Tech Stack:** React 19、TypeScript、Vite、Vitest、Testing Library、Tauri 2、Rust、Tokio、redis crate、serde/serde_json、系统 keyring。

**Spec:** `docs/superpowers/specs/2026-08-18-redisinsight-non-cloud-parity-design.md`

## Global Constraints

- 默认继续在当前 `main` 分支工作，不创建分支。
- 所有提交信息使用简体中文。
- 继续使用 Redis `SCAN` 分页，不执行阻塞式 `KEYS`；不在循环遍历中执行 SQL。
- 密码只进入系统钥匙串；普通 JSON、历史、日志、错误和 UI 状态不得保存密码。
- Redis 底层错误映射为固定错误码，不向前端返回 URI、密码或原始错误文本。
- Redis Cloud、Azure、AI、Telemetry、远程插件、Monaco 插件运行时和本地 HTTP 服务不进入产品源码。
- 每个任务必须先写行为测试并确认 RED，再实现最小 GREEN，再运行全量相关回归。
- 每个任务结束都运行 `git diff --check`，提交前运行对应计划的完整验证命令。

## 子计划与顺序

| 顺序 | 子计划 | 独立交付物 | 依赖 |
|---|---|---|---|
| 1 | `2026-08-18-browser-productivity.md` | Browser 搜索/类型过滤/元数据/本地导入导出，并建立版本化 JSON 文档仓储 | 现有 MVP |
| 2 | `2026-08-18-workbench-enhancement.md` | 命令目录、多命令执行、结果格式、复制、敏感历史持久化 | 子计划 1 的 JSON 文档仓储；现有 Workbench |
| 3 | `2026-08-18-database-overview.md` | 实例概览、数据库概览、数据库切换和导航入口 | 现有连接模型；可与子计划 2 顺序执行 |
| 4 | `2026-08-18-query-library-settings.md` | Query Library、设置、原子 JSON 存储和导航入口 | 子计划 1 的 JSON 文档仓储；子计划 2 的 Workbench 回填接口 |

## 总体验收矩阵

| 需求 | 覆盖子计划 | 主要验证 |
|---|---|---|
| Browser pattern、类型过滤、刷新、元数据 | 1 | `browserState.test.ts`、`browser.test.tsx`、Rust domain/integration |
| Browser 本地导入导出 | 1 | DTO/service 单测、组件 file input/Blob 测试、真实 Redis 流程 |
| 命令目录与补全 | 2 | 命令目录纯函数测试、Workbench component 测试 |
| 多命令、停止策略、格式化、复制 | 2 | Rust execute batch 测试、Workbench component 测试 |
| 非敏感历史持久化 | 2 | history filter + JSON store tests、连接隔离回归 |
| Redis/数据库概览 | 3 | Rust aggregate parser/service 测试、前端 loading/error/empty 测试 |
| 数据库切换 | 3 | Redis integration + connection state regression |
| Query Library | 4 | JSON persistence、command CRUD、UI search/edit/delete/fill tests |
| Settings | 4 | version migration、atomic write、UI preference tests |
| Cloud 排除 | 1-4 | `npm run check:non-cloud`、源码 diff review |

## 每个子计划的交付协议

1. 开始子计划前读取本总计划、对应子计划、spec、`findings.md`、`progress.md`。
2. 按子计划任务顺序执行，不把后续子计划的功能提前混入当前 diff。
3. 每个任务按测试失败 → 最小实现 → 测试通过 → 重构 → 中文 commit 完成。
4. 子计划完成后运行前端、Rust、集成和范围检查，并把实际结果写入 `progress.md`。
5. 如果 Redis 模块或本地 Redis 不可用，保留稳定降级测试；不得将环境限制伪装成通过。

## 最终验证

所有子计划完成后运行：

```bash
npm run check:non-cloud
npm run test:frontend
npm run build
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo test --manifest-path src-tauri/Cargo.toml
git diff --check
```

本机 Redis 可用时再运行：

```bash
REDIX_TEST_REDIS_URL=redis://127.0.0.1:6379 cargo test --manifest-path src-tauri/Cargo.toml --test redis_integration -- --ignored --nocapture
```

完成条件是四个能力批次均可从应用入口进入，已有连接/Browser/Workbench 回归通过，敏感信息不落盘，非 Cloud 扫描通过，且所有计划任务勾选完成。
