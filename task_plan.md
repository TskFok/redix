# Task Plan: RedisInsight 功能的 Rust Tauri 重构

## Goal

在当前空目录中创建一个基于 Rust + Tauri 的 Redis 桌面客户端，复用 RedisInsight 的本地 Redis 管理核心体验，并明确排除 Redis Cloud 相关功能。

## Current Phase

Phase 11：本地 Redis Stream Consumer Group（已完成）

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
| 首次用 JS 字符串写实现计划时引号未转义触发 SyntaxError | 1 | 改用 String.raw 和 fenced plan patch 重试，随后完成计划自审 |

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

已完成：Slow Log / Pub/Sub 的 Rust service、Tauri commands、typed bridge 和前端工作区已交付，并完成全量回归与交付审查。

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
| 追加 Task 17 记录的首次补丁上下文未命中 | 1 | 重新读取文件尾部并使用精确末行重新应用补丁 |

## Phase 10：本地 Redis Profiler（2026-08-24）

### Goal

对照 RedisInsight 的 Profiler/Monitor 能力，补齐本地 Standalone Redis 的独立 `MONITOR` 实时命令流、停止和前端展示；不引入日志文件、拓扑 fan-out、TLS/SSH、模块或任何云功能。

### Status

已完成基础 Profiler 实现、前端工作区、测试和范围文档；日志文件、历史持久化、拓扑 fan-out、TLS/SSH、模块和云能力仍明确排除。

### Artifacts

- 设计：`docs/superpowers/specs/2026-08-24-profiler-design.md`
- 计划：`docs/superpowers/plans/2026-08-24-profiler.md`

### Checklist

- [x] Profiler DTO、输入校验和 MONITOR 文本解析
- [x] 独立 MONITOR socket、任务停止和连接生命周期清理
- [x] Tauri commands、事件注册和 typed bridge
- [x] 运维观察 Profiler tab、性能警告、事件列表和 10000 条缓存上限
- [x] 前端/Rust/构建/非 Cloud/差异回归验证

### Delivery

- `413f8dc`：增加 Profiler 领域模型和 MONITOR 解析
- `056b31c`：实现 Profiler Tauri 命令桥接
- `39aa81b`：接入 Profiler 前端实时工作区

## Phase 11：本地 Redis Stream Consumer Group（2026-08-24）

### Goal

对照 RedisInsight Stream Browser 能力，在本地 Redis Standalone 的 Stream 键详情中补齐 Consumer Group、消费者、PENDING 观察、XACK 和消费者删除；不实现实时消费、Claim、拓扑、TLS/SSH、模块或任何云功能。

### Status

已完成领域解析、Redis service、Tauri bridge、Browser 工作区和全量回归；真实 Redis Consumer Group 流程保留为默认 ignored 集成测试。

### Artifacts

- 设计：`docs/superpowers/specs/2026-08-24-stream-consumer-groups-design.md`
- 计划：`docs/superpowers/plans/2026-08-24-stream-consumer-groups.md`

### Checklist

- [x] Stream Consumer Group DTO、输入校验和 RESP parser
- [x] Redis service 与 XINFO/XGROUP/XPENDING/XACK 命令
- [x] Tauri command 和 TypeScript bridge
- [x] Stream 键详情 Consumer Groups UI
- [x] Rust/前端/非 Cloud/构建/差异回归验证

### Delivery

- `bdf0460`：增加 Stream Consumer Group 领域模型和解析
- `54f592b`：实现 Stream Consumer Group Redis 服务
- `5c0706f`：接入 Stream Consumer Group Tauri 桥接
- `d21ca27`：增加 Stream Consumer Group Browser 工作区

## Phase 12：RedisInsight 非 Cloud 功能全量补齐

- [x] 恢复既有计划、发现和进度记录，确认已完成能力与工作区状态
- [x] 只读盘点目标 RedisInsight 的 README、UI 页面、API 模块和当前 Redix 的实现面
- [x] 将缺口按通用工作区、连接栈、拓扑安全、模块专用能力分批
- [x] 在聊天中确认本轮推荐设计和执行批次
- [x] 编写并自审本轮 Database Analysis/Instance 设计文档
- [x] 用户审阅本轮设计文档
- [x] 编写并自审本轮实现计划
- [x] 用户选择并执行 Database Analysis/Instance 批次
- [x] 按 TDD 完成 Database Analysis/Instance 的领域、服务、typed IPC、前端与 ignored Standalone Redis 流程
- [x] 完成 Database Analysis/Instance 的前端、Rust、构建、非 Cloud 和 ignored Redis 验证
- [ ] 后续连接栈：连接导入导出、TLS/证书、SSH 隧道
- [ ] 后续拓扑：Sentinel、Cluster 与拓扑安全/命令路由
- [ ] 后续模块编辑器：完整 RedisJSON、Search/Query、Vector Set、Array 与其他模块类型
- [ ] 后续 Workbench 高级命令帮助、补全与结果可视化
- **Status:** in_progress（Database Analysis/Instance 子批次已完成；TLS/SSH、拓扑、模块编辑器和 Workbench 高级能力仍未完成）

### Database Analysis/Instance 最终集中修复波次（2026-08-24）

- [x] 核验 5 个最终评审 finding 与现有实现的数据流和测试覆盖
- [x] 按 TDD 修复 commandstats 显式读取、复制字段映射与 INFO 可选字段局部降级
- [x] 按 TDD 修复 SCAN 最终页/非最终页上限语义、max_keys 上边界与删除竞态过滤
- [x] 按 TDD 阻止 loading 期间重复提交，同时保留连接切换旧响应保护
- [x] 运行 Rust domain/service/commands、前端页面、全量构建、非 Cloud、fmt、diff 验收
- [x] 写入 final-fix-report.md，并创建一个简体中文修复提交
- **Status:** complete

## Phase 13：连接配置导入导出与 Standalone TLS/证书

### Goal

参照 `/Users/ushopal/workspace/myself/RedisInsight`，在当前 Redix 的本地 Redis Standalone 连接管理中补齐连接配置导入/导出，以及 CA/客户端证书/私钥和 TLS 连接能力；保持 Rust + Tauri + React、typed IPC、固定错误码和密码/私钥不落普通 JSON 的安全边界，不引入 Redis Cloud、SSH、Sentinel、Cluster 或 SQL。

### Status

已完成：按 TDD 实现领域/持久化/连接/IPC/UI，并完成全量验证与交付记录。

### Initial checklist

- [x] 精读目标项目连接导入导出、证书导入/校验和 Standalone TLS 连接链路
- [x] 精读当前 profile、secret store、Redis client、Tauri command、typed bridge 和连接表单
- [x] 明确导出文件的敏感字段、导入冲突/校验/路径策略和 TLS 证书持久化边界
- [x] 与用户确认架构设计和验收范围
- [x] 编写并自审设计文档，等待用户审阅
- [x] 编写实现计划并按 TDD 实现领域/持久化/连接/IPC/UI
- [x] 完成 Rust、前端、构建、非 Cloud、格式和差异检查
- [x] 在无 TLS Redis 证书环境时明确保持真实 TLS 集成未声称通过
- [x] 写入 `.superpowers/sdd/2026-08-24-connection-import-export-standalone-tls/final-report.md`
- [x] 使用简体中文提交最终验证记录

### Scope boundaries

- 只支持 Standalone TCP/TLS；不实现 SSH 隧道、Sentinel、Cluster 拓扑或 Redis Cloud。
- 连接导出默认只导出可迁移的 profile 元数据与 TLS 开关/校验配置，绝不写入密码、CA PEM、客户端证书或私钥；导入后敏感材料需在当前设备重新录入。
- 不使用 SQL；任何后续遍历均不得在循环中查询 SQL。

## Phase 14：当前项目与 RedisInsight 的剩余非 Cloud 差异补全（2026-08-24）

### Goal

在已有 Standalone、观察、分析、连接导入导出和 TLS 能力之上，继续对照 `/Users/ushopal/workspace/myself/RedisInsight` 补齐可归属于本地 Redis 的缺失功能；Redis Cloud、Azure Managed Redis、RDI、AI/Copilot、Telemetry、远程插件和 SQL 不进入实现范围。

### Status

discovery：正在按连接拓扑、模块能力、Workbench/CLI 高级体验和文档一致性盘点剩余差异，待设计确认后进入分批实现。

### Initial checklist

- [x] 恢复既有计划、发现、进度和已交付功能清单
- [x] 读取当前/目标仓库状态、入口、页面和 API 模块分布
- [x] 记录 README 与当前 TLS 实现不一致的问题
- [x] 精读剩余本地能力的目标数据流、命令和 UI 验收边界
- [x] 与用户确认本轮分批设计和优先级
- [x] 编写并自审设计文档
- [x] 用户审阅设计文档
- [x] 编写并自审第一批实现计划
- [ ] 选择执行方式
- [ ] 按 TDD 分批实现并验证

### Scope boundaries

- 不实现 Redis Cloud、Azure Managed Redis、RDI、AI/Copilot、Telemetry、远程插件、云登录/云账户/云端点/云数据库发现和 SQL。
- 默认继续在当前 `main` 分支修改，不创建分支；提交信息使用简体中文。
- 保持 Redis 访问为 typed IPC 和固定错误映射；不在循环遍历中查询 SQL，不引入 SQL 持久化。

### Artifacts

- 设计文档：`docs/superpowers/specs/2026-08-24-redisinsight-non-cloud-parity-modules-topology-design.md`
- 第一批实施计划：`docs/superpowers/plans/2026-08-24-redisjson-module-capabilities.md`

### Current status

plan_ready：设计文档已获用户确认并提交；第一批 RedisJSON/模块能力实施计划已完成自审，等待用户选择执行方式。

### Discovery errors

| Error | Attempt | Resolution |
|-------|---------|------------|
| 首次追加规划补丁的旧 findings 上下文不匹配 | 1 | 先读取真实文件尾部，再用精确上下文追加，未改动业务源码 |
| 只读命令引用不存在的 `src/features/browser/Browser.tsx` | 1 | 后续按实际文件清单定位 Browser 入口，未改动业务源码 |

### Task 5：范围文档与第一批完整验证（2026-08-26）

- [x] 修正 `README.md` 中过时的 TLS 排除描述，补充 Standalone TCP/TLS、RedisJSON path 读写/删除/数组追加、`MODULE LIST` 能力探测与降级说明
- [x] 补齐 `docs/non-cloud-scope.md` 的允许项/排除项，记录 Redis Stack ignored 集成环境变量 `REDIX_TEST_REDIS_STACK_URL` 与未配置时必须记为 skip
- [x] 运行 `npm run test:frontend`、`npm run build`、`npm run check:non-cloud`、`npm run test:rust`、`cargo fmt --manifest-path src-tauri/Cargo.toml -- --check`、`git diff --check`
- [x] 明确记录 `REDIX_TEST_REDIS_STACK_URL` 未配置，因此 Redis Stack ignored 流程保持未执行，不声称真实网络通过
- [x] 写入 `.superpowers/sdd/2026-08-24-redisjson-module-capabilities/task-5-report.md`
- [x] 创建简体中文提交
- **Status:** complete（本任务只更新范围/记录文档与验证证据；未修改业务实现文件。`cargo fmt --check` 因既有业务文件格式差异失败，已如实记录）
