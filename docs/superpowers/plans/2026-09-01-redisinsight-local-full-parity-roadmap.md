# RedisInsight 本地功能全量对齐实施路线图

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 按依赖顺序完成 RedisInsight 除明确排除项之外的本地功能对齐，并让每个批次独立形成可运行、可测试的软件闭环。

**Architecture:** 继续使用 Rust + Tauri typed IPC + React。先稳定连接目标、节点作用域和跨平台传输，再依次扩展 Browser、Search/Workbench、分析、本地产品能力和平台收口；每批验收后才冻结下一批所依赖的接口。

**Tech Stack:** Rust 2021、Tauri 2、redis 1.5、Tokio、React 19、TypeScript 5.8、Vitest、Python 隔离测试脚本、GitHub Actions。

**Spec:** `docs/superpowers/specs/2026-09-01-redisinsight-local-full-parity-design.md`

## Global Constraints

- 永久排除 Redis Cloud、Azure Managed Redis、RDI、AI/Copilot、Telemetry/Analytics 和远程插件。
- 对齐用户可见工作流、Redis 命令语义、安全边界和错误行为，不复制 Electron/NestJS 内部实现。
- macOS、Windows、Linux 均进入实现与验证矩阵；区分本机、CI 和实机验证。
- 不使用 `KEYS`，不引入 SQL，不在循环遍历中查询 SQL。
- 凭据、私钥、证书正文和敏感路径不得进入 profile JSON、普通导出、日志或 IPC 错误。
- 所有扫描、解析、fan-out、解码、任务和 IPC 结果必须有固定上限。
- 默认在当前 `main` 分支工作，不擅自创建分支；提交信息使用简体中文。
- 参考项目 `/Users/ushopal/workspace/myself/RedisInsight` 只读。

---

## 批次顺序与计划门槛

| 顺序 | 批次 | 可独立交付的结果 | 详细计划门槛 |
|---:|---|---|---|
| 1 | 连接与拓扑 | Cluster、三平台 SSH、合法传输组合、跨节点扫描/详情/分析 | 已编写：`2026-09-01-connection-topology.md` |
| 2 | Browser 深化 | 本地值解码、后台批量任务、Stream 实时消费/XAUTOCLAIM | 批次 1 的 `ConnectionTarget`、`NodeScope`、`RoutedConnection` 验收后编写 |
| 3 | Search、Vector 与 Workbench | 高级索引/向量流程、查询构建、本地内置可视化 | 批次 2 的任务/解码/结果上限合同验收后编写 |
| 4 | 分析与推荐 | 后台分析、时间序列、可解释本地推荐 | 批次 3 的查询/结果 DTO 稳定后编写 |
| 5 | 本地产品能力 | 标签、查询包、快捷键、通知、跨平台自动更新 | 批次 4 的本地任务与资源版本合同稳定后编写 |
| 6 | 跨平台收口 | 三平台构建、打包、桌面 E2E、最终功能矩阵 | 前五批均通过各自验收后编写 |

后续计划不提前猜测会被前置批次改变的签名。每个批次开始前重新对照设计、功能矩阵和参考项目当前 HEAD，编写与自审独立计划，再进入 TDD。

## 批次级完成门槛

- [ ] 该批次的 Rust domain/service/command 与 TypeScript DTO/bridge 契约一致。
- [ ] 成功、失败、取消、竞态、部分失败和能力降级路径有测试。
- [ ] 隔离 Redis/Redis Stack/拓扑环境按需要真实执行；缺失环境明确记录 skip。
- [ ] 前端全量测试、Rust 全量测试、生产构建、non-cloud、格式和 diff 检查通过。
- [ ] 支持声明与 README、`docs/non-cloud-scope.md`、功能矩阵一致。
- [ ] 使用简体中文提交，不自动 push、发布、签名或安装。
