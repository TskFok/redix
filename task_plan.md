# Task Plan: RedisInsight 功能的 Rust Tauri 重构

## Goal

在当前空目录中创建一个基于 Rust + Tauri 的 Redis 桌面客户端，复用 RedisInsight 的本地 Redis 管理核心体验，并明确排除 Redis Cloud 相关功能。

## Current Phase

Phase 3：Task 4 Redis 核心连接服务已实现，等待任务级审查

## Phases

### Phase 1：需求与范围确认

- [x] 检查当前工作区与参考项目结构
- [x] 识别 RedisInsight 的非 Cloud 核心能力
- [x] 确认第一阶段 MVP 范围与成功标准
- **Status:** complete

### Phase 2：设计与项目结构

- [x] 提出 2-3 种 Tauri 架构方案并确认取舍
- [x] 编写并自审设计文档
- [x] 等待用户确认设计文档
- **Status:** complete

### Phase 3：实现

- [ ] 按测试驱动方式先写可失败的行为测试
- [ ] 实现 Rust 核心命令与本地持久化
- [ ] 实现 Tauri 前端界面与核心交互
- [ ] 加入 Redis Cloud 功能排除边界
- **Status:** in_progress

#### 当前 Task 4 执行清单

- [x] 核对计划、领域模型、错误、持久化及 Task 3 已实现边界
- [x] 以现有 tokenizer/URL/TTL 测试骨架作为 RED 基线实现 `RedisService`
- [x] 运行指定离线 Redis 单元测试并根据真实编译错误修复
- [x] 添加可选本地 Redis 五类型集成测试
- [x] 执行 fmt、领域测试、差异审查并写 Task 4 报告
- [x] 使用简体中文提交 Task 4

### Phase 4：测试与验证

- [ ] 运行 Rust 单元测试与前端检查
- [ ] 构建 Tauri 开发/生产包
- [ ] 验证本地 Redis 连接、键浏览、命令执行与错误提示
- [ ] 对照需求逐项检查 Cloud 功能未进入产品
- **Status:** pending

### Phase 5：交付

- [ ] 审查变更与运行说明
- [ ] 使用简体中文提交 commit（如需要提交）
- [ ] 向用户交付文件路径与验证结果
- **Status:** pending

## Key Questions

1. MVP 需要覆盖：Redis Standalone 连接、键浏览/过滤、基础 CRUD、Workbench 命令执行和连接配置持久化。
2. Cluster、Sentinel、TLS/SSH、Profiler、Slow Log、Pub/Sub、JSON/Search 等能力不进入首个交付闭环。
3. 前端技术选择应在开发效率、依赖规模和 Rust/Tauri 集成复杂度之间平衡。

## Decisions Made

| Decision | Rationale |
|----------|-----------|
| 在当前分支/目录直接工作 | 用户明确要求默认当前分支，且当前目录为空 |
| 采用分阶段交付 | RedisInsight 的完整非 Cloud 能力跨越连接管理、数据浏览、命令工作台、监控和插件等多个子系统 |
| 排除 Redis Cloud、云账户、云 API、云数据库发现和云登录 | 用户明确要求不包含 Redis Cloud 相关功能 |
| 首个 MVP 聚焦 Standalone + Browser + Workbench | 先形成可运行的端到端闭环，减少一次性移植大型产品的风险 |

## Errors Encountered

| Error | Attempt | Resolution |
|-------|---------|------------|
| Redis 1.5 `Value::BigNumber` 是 `num_bigint::BigInt`，不存在 `as_ref()` | 1 | 改用安全的 `to_string()` 转换，不暴露 Redis 错误文本 |
| 当前目录不是 Git 仓库 | 1 | 记录为当前工作区状态；待设计确认后再初始化项目与 Git 元数据 |
| 规划文件补丁上下文不匹配 | 1 | 重新读取当前文件后，用精确上下文更新 |
| `cargo info` 无法解析 crates.io | 1 | 记录错误；依赖安装阶段根据需要申请网络权限，当前使用 `cargo search` 已返回的版本信息 |

## Notes

- 参考项目：`/Users/ushopal/workspace/myself/RedisInsight`
- 当前目标目录：`/Users/ushopal/workspace/myself/redix`
- 当前目标目录初始为空。
- 设计文档已写入并提交：`docs/superpowers/specs/2026-08-14-redix-tauri-design.md`，等待用户审阅后进入实现计划。
- 实现计划已写入：`docs/superpowers/plans/2026-08-14-redix-tauri.md`，用户已选择子代理驱动执行。
- 用户已确认将最小 Tauri 图标资源前移到 Task 1，以便 Rust 测试可以独立编译；Task 10 继续负责正式 bundle 配置与最终非 Cloud 审查。
