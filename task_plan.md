# Task Plan: RedisInsight 功能的 Rust Tauri 重构

## Goal

在当前空目录中创建一个基于 Rust + Tauri 的 Redis 桌面客户端，复用 RedisInsight 的本地 Redis 管理核心体验，并明确排除 Redis Cloud 相关功能。

## Current Phase

Phase 8：RedisInsight 非 Cloud 差异补全（第一批已完成）

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

- [x] 按测试驱动方式先写可失败的行为测试
- [x] 实现 Rust 核心命令与本地持久化
- [x] 实现 Tauri 前端界面与核心交互
- [x] 加入 Redis Cloud 功能排除边界
- **Status:** complete

#### 当前 Task 4 执行清单

- [x] 核对计划、领域模型、错误、持久化及 Task 3 已实现边界
- [x] 以现有 tokenizer/URL/TTL 测试骨架作为 RED 基线实现 `RedisService`
- [x] 运行指定离线 Redis 单元测试并根据真实编译错误修复
- [x] 添加可选本地 Redis 五类型集成测试（普通运行明确 `ignored`；设置 `REDIX_TEST_REDIS_URL` 后须传 `--ignored --nocapture` 执行）
- [x] 执行 fmt、领域测试、差异审查并写 Task 4 报告（真实 Redis 流受环境限制时如实记录）
- [x] 使用简体中文提交 Task 4
- **Status:** complete（修复复审 PASS）

#### 当前 Task 5 执行清单

- [x] 先写 Tauri 命令层行为测试，覆盖 AppState、无效配置和密钥写入边界
- [x] 实现连接配置、Browser、Workbench 命令及统一错误映射
- [x] 验证命令层不暴露密码、不执行 SQL/Cloud 逻辑
- [x] 运行 Rust 命令层测试、fmt 和差异审查并写 Task 5 报告
- [x] 使用简体中文提交 Task 5
- **Status:** complete（修复复审 PASS）

#### 当前 Task 6 执行清单

- [x] 先写前端 IPC bridge 失败测试并确认失败原因
- [x] 实现与 Rust DTO 对齐的 TypeScript 类型和 12 个 typed wrappers
- [x] 统一保留 `{ code, message }` 的 IPC 错误形态，不暴露密码
- [x] 运行 bridge 测试、前端生产构建并写 Task 6 报告
- [x] 使用简体中文提交 Task 6
- **Status:** complete（任务审查 PASS）

#### 当前 Task 7 执行清单

- [x] 先写连接页面失败测试，覆盖空连接列表、表单和保存/连接动作
- [x] 实现连接状态、连接列表、表单校验和错误提示
- [x] 实现保存、测试连接、打开连接、删除连接的交互顺序
- [x] 运行前端连接测试和生产构建并写 Task 7 报告
- [x] 使用简体中文提交 Task 7
- **Status:** complete（修复复审 PASS）

#### 当前 Task 8 执行清单

- [x] 先写 Browser 列表、详情和五种数据编辑器失败测试
- [x] 实现 SCAN 分页/过滤、键详情读取、CRUD、TTL 和删除交互
- [x] 实现 String、Hash、List、Set、Sorted Set 编辑器与可访问状态反馈
- [x] 运行 Browser 测试、全量前端测试和生产构建并写 Task 8 报告
- [x] 使用简体中文提交 Task 8
- **Status:** complete（修复复审 PASS）

#### 当前 Task 9 执行清单

- [x] 先写 Workbench 命令输入、执行、结果和历史测试
- [x] 实现 PING/GET/SET 等命令执行 bridge 调用及结构化结果展示
- [x] 实现 Cmd/Ctrl+Enter、loading、错误状态和历史回填
- [x] 运行 Workbench 测试、全量前端测试和生产构建并写 Task 9 报告
- [x] 使用简体中文提交 Task 9
- **Status:** complete（修复复审 PASS）

#### 当前 Task 10 执行清单

- [x] 先写非 Cloud 范围扫描失败测试并确认失败原因
- [x] 实现范围扫描脚本、深浅主题、焦点态、正式 Tauri bundle 配置和图标引用
- [x] 更新 README、npm scripts、非 Cloud 范围文档与验证记录
- [x] 运行非 Cloud 扫描、前端测试/构建、Rust 测试、可用时的 Redis 集成测试、Tauri 构建和差异检查
- [x] 对照 Redis Cloud 排除项、Browser `SCAN` 约束、密码持久化边界和 SQL 约束进行最终静态审查
- [x] 使用简体中文提交最终 MVP
- **Status:** complete（真实 Redis 集成和沙箱外 Tauri bundle 均通过）

### Phase 4：测试与验证

- [x] 运行 Rust 单元测试与前端检查
- [x] 构建 Tauri 开发/生产包
- [x] 验证本地 Redis 连接、键浏览、命令执行与错误提示
- [x] 对照需求逐项检查 Cloud 功能未进入产品
- **Status:** complete

### Phase 5：交付

- [x] 审查变更与运行说明
- [x] 使用简体中文提交 commit（如需要提交）
- [x] 向用户交付文件路径与验证结果
- **Status:** complete

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
| 追加 Phase 7 记录的首次补丁未命中 | 1 | 拆分为精确定位的小补丁后重新追加，未覆盖历史内容 |
| Task 1 RED 命令多写测试筛选词 | 1 | 改用 `--lib` 与 `--test domain` 两个独立命令执行聚焦测试 |
| `cargo info` 无法解析 crates.io | 1 | 记录错误；依赖安装阶段根据需要申请网络权限，当前使用 `cargo search` 已返回的版本信息 |
| 沙箱内 Redis 集成测试无法访问 `127.0.0.1` | 1 | 使用授权的沙箱外命令确认 Redis 返回 `PONG`，随后集成测试 1/1 通过 |

## Notes

- 参考项目：`/Users/ushopal/workspace/myself/RedisInsight`
- 当前目标目录：`/Users/ushopal/workspace/myself/redix`
- 当前目标目录初始为空。
- 设计文档已写入并提交：`docs/superpowers/specs/2026-08-14-redix-tauri-design.md`，等待用户审阅后进入实现计划。
- 实现计划已写入：`docs/superpowers/plans/2026-08-14-redix-tauri.md`，用户已选择子代理驱动执行。
- 用户已确认将最小 Tauri 图标资源前移到 Task 1，以便 Rust 测试可以独立编译；Task 10 继续负责正式 bundle 配置与最终非 Cloud 审查。

## Phase 6：RedisInsight 视觉与排版对齐（2026-08-18）

### Goal

在不改变 Redis MVP 业务边界和现有 Rust 连接逻辑的前提下，参考 `/Users/ushopal/workspace/myself/RedisInsight` 的桌面端界面层次、色彩、密度和工作区排版，统一改造当前 React 前端。

### Phases

- [x] 勘察当前三类页面与 RedisInsight 对应页面的结构、样式 token 和交互状态
- [x] 在聊天中确认视觉改造设计与范围
- [x] 实现全局视觉 token、应用壳层、连接页、Browser 和 Workbench 的排版对齐
- [x] 使用浏览器/构建检查验证桌面宽度、窄窗口、默认主题和无障碍状态
- [x] 审查差异，记录未修改的用户 Rust 变更并交付
- **Status:** complete

### Task 11 实施记录

- `src/App.tsx` 改为左侧工作区导航、顶部连接上下文和单一全高工作区；未连接时 Browser/Workbench 保持禁用。
- `src/styles.css` 按 RedisInsight 参考色板重建深浅主题 token、间距、边框、按钮、表单、反馈、Browser 双栏和 Workbench 工作区样式，并补充窄屏断点。
- `src/app.smoke.test.tsx` 增加默认入口和禁用工作区回归测试；修复应用壳层新增标题与 Workbench 标题重复的无障碍查询冲突。
- 应用内浏览器已检查默认桌面视口、375px 窄窗口和新增连接表单；移动端未发现横向溢出，检查后已恢复默认视口。
- 当前工作区中的 `src-tauri/Cargo.toml`、`src-tauri/src/commands/connections.rs`、`src-tauri/tests/commands.rs` 为用户既有未提交变更，本轮未修改。
- 验证命令：`npm run test:frontend`、`npm run build`、`git diff --check`。

### Constraints

- 默认继续在 `main` 分支直接工作，不创建新分支。
- 不修改现有 `src-tauri/` 用户变更，不引入 Redis Cloud、SQL 查询或新的业务能力。
- 只在用户批准视觉设计后开始实现代码。
- 计划、发现和进度文件持续记录本轮工作，不覆盖上一轮 MVP 记录。

## Phase 7：Browser 能力扩展（2026-08-18）

### Goal

在本地 Redis Standalone 上补齐 RedisInsight Browser 的首批高频能力：新增键、重命名、批量删除、元数据刷新，并支持基础 Stream 与 RedisJSON 根文档编辑；Redis Cloud、Azure、AI、Telemetry、远程插件和长连接运维能力不进入本阶段。

### Status

已完成 Browser 首批非 Cloud 能力扩展、真实 Redis 验证、范围审查和文档交付。

### Artifacts

- 设计：docs/superpowers/specs/2026-08-18-browser-capability-expansion-design.md
- 计划：docs/superpowers/plans/2026-08-18-browser-capability-expansion.md

### Checklist

- [x] 扩展 Rust 领域 DTO、Stream/JSON 编解码和错误边界
- [x] 实现新增、重命名、批量删除、元数据和 Stream/JSON Redis 操作
- [x] 实现 Tauri bridge 与 Browser 新增/选择/批量 UI
- [x] 实现详情重命名、元数据刷新和 Stream/JSON 编辑器
- [x] 完成前端、Rust、真实 Redis 和非 Cloud 验证

### 交付记录

- 前端：Browser 测试 27/27，全量前端测试 69/69，生产构建通过。
- Rust：离线测试 27 个库测试、2 个命令测试、9 个领域测试、9 个持久化测试通过；Redis 集成测试 1/1 通过，另有 1 个 ignored 测试；`cargo fmt --check` 通过。
- 真实 Redis：`redis://127.0.0.1:6379` 集成流程通过；无 RedisJSON 时返回稳定的 `UNSUPPORTED_DATA_TYPE`。
- 范围：非 Cloud 扫描通过；未引入 SQL、Redis Cloud、Azure、AI、Telemetry 或远程插件入口。
- 保护：保留用户原有未提交文件和修改，未将其纳入本轮文档提交。

## Phase 8：RedisInsight 非 Cloud 差异补全（2026-08-18）

### Goal

在不引入 Redis Cloud、Azure 云接入、AI、Telemetry 或远程插件能力的前提下，对照 `/Users/ushopal/workspace/myself/RedisInsight` 的本地 Redis 功能，按优先级继续补齐当前 Redix 缺失的可交付能力，并保持 Rust/Tauri/React 架构与测试边界稳定。

### Status

第一批完成：Browser 生产力、Workbench 增强、数据库/实例概览和 Query Library/Settings 已实现并通过总体验收；Slow Log、Pub/Sub、Profiler、拓扑/安全连接和模块专用能力仍按后续批次单独规划。

### 已确认设计决策

- 用户已确认继续使用 React + Vite + Tauri 2 + Rust，并按 typed IPC 和功能批次扩展。
- 第一批顺序为 Browser 生产力、Workbench 增强、数据库/实例概览、Query Library/设置。
- 本地持久化继续使用 JSON 文件和系统钥匙串，不引入 SQL；Redis Cloud、Azure、AI、Telemetry、远程插件不进入产品源码。
- Slow Log 先使用 request-response IPC；Pub/Sub/Profiler 后续使用可取消的 Tauri event channel；TLS/SSH/Sentinel/Cluster 单独扩展。
- 第一批按 Browser 生产力、Workbench 增强、数据库/实例概览、Query Library/设置拆分；Browser 保持 SCAN 分页，Workbench 先不引入 Monaco/插件运行时，数据库概览使用聚合只读 DTO，本地资源使用版本化 JSON 原子写入。
- 统一使用固定错误码；通过 `COMMAND INFO`/`MODULE LIST` 探测能力，模块不可用时局部降级；密码只进系统钥匙串，敏感命令不写历史，连接切换和页面卸载取消旧请求。

### Checklist

- [x] 盘点目标项目当前版本的非 Cloud 页面、API、数据类型和本地设置能力
- [x] 盘点当前项目已有入口、IPC、Redis service、持久化和测试缺口
- [x] 将缺失能力拆为可独立验收的批次，明确排除项和依赖关系
- [x] 与用户确认推荐实施范围
- [x] 与用户确认详细架构设计
- [x] 编写并自审正式设计文档
- [x] 等待用户审阅正式设计文档
- [x] 编写四个按依赖顺序执行的详细实现计划并完成自审
- [x] 依照计划按 TDD 实现并验证 Browser 生产力批次
- [x] 依照计划按 TDD 实现并验证 Workbench 增强批次
- [x] 依照计划按 TDD 实现并验证数据库/实例概览批次
- [x] 依照计划按 TDD 实现并验证 Query Library/Settings 批次

### Phase 8 计划产物

- 设计：`docs/superpowers/specs/2026-08-18-redisinsight-non-cloud-parity-design.md`
- 总计划：`docs/superpowers/plans/2026-08-18-redisinsight-non-cloud-parity.md`
- 子计划 1：`docs/superpowers/plans/2026-08-18-browser-productivity.md`
- 子计划 2：`docs/superpowers/plans/2026-08-18-workbench-enhancement.md`
- 子计划 3：`docs/superpowers/plans/2026-08-18-database-overview.md`
- 子计划 4：`docs/superpowers/plans/2026-08-18-query-library-settings.md`

执行顺序固定为 Browser → Workbench → Database → Query Library/Settings；每个子计划结束后单独验证并提交，最终再执行总体验收矩阵。

## Phase 9：本地 Redis 运维观察能力（2026-08-24）

### Goal

对照目标 RedisInsight 的非 Cloud Slow Log 与 Pub/Sub 能力，在当前 Redix 中实现本地 Standalone Redis 的慢命令读取/配置/清空，以及可取消的频道/模式订阅和发布消息；不引入 Profiler、MONITOR、Cluster、Sentinel、TLS、SSH、模块专用能力或任何云功能。

### Status

进行中：Slow Log / Pub/Sub 的 Rust service、Tauri commands、typed bridge 和前端工作区已完成，当前进行全量回归与交付审查。

### Artifacts

- 设计：`docs/superpowers/specs/2026-08-24-observability-design.md`
- 计划：`docs/superpowers/plans/2026-08-24-observability.md`

### Checklist

- [x] DTO、校验、Slow Log RESP 解析和固定错误码
- [x] Slow Log service 与 Pub/Sub 独立 socket 生命周期
- [x] Tauri commands、事件注册和 typed bridge
- [x] Slow Log React 工作区
- [x] Pub/Sub React 工作区与 listener/session 清理
- [x] 导航、响应式样式、Smoke 和边界文档
- [x] 前端/Rust/非 Cloud/差异总体验收（真实清空型集成测试保留为 ignored）

### Current Task

- [x] Task 1：DTO、校验与 Redis reply 解析
- [x] Task 2：Redis Slow Log 与 Pub/Sub service 生命周期
- [x] Task 3：Tauri command 注册与 typed bridge
- [x] Task 4：Slow Log React 状态与页面
- [x] Task 5：Pub/Sub 状态、事件监听与页面
- [x] Task 6：应用导航、样式、Smoke 与文档边界
- [x] Task 7：总体验收、非 Cloud 验证与交付审查

### Errors Encountered

| Error | Attempt | Resolution |
|---|---:|---|
| 编排读取文件的 JS 字符串转义导致 `SyntaxError` | 1 | 将命令字符串拆分为简单字符串后重跑，未修改文件、未影响计划内容 |

## Phase 10：本地 Redis Profiler（2026-08-24）

### Goal

对照 RedisInsight 的 Profiler/Monitor 能力，补齐本地 Standalone Redis 的独立 `MONITOR` 实时命令流、停止和前端展示；不引入日志文件、拓扑 fan-out、TLS/SSH、模块或任何云功能。

### Status

已完成 spec 与实施计划，准备按 TDD 执行。

### Artifacts

- 设计：`docs/superpowers/specs/2026-08-24-profiler-design.md`
- 计划：`docs/superpowers/plans/2026-08-24-profiler.md`
