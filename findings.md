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

- 已通过应用内浏览器检查本地 Vite 页面：默认桌面视口显示固定左侧导航、顶部连接上下文和全高内容区；未连接时 Browser/Workbench 的禁用状态与视觉层级正常。
- 已将视口切换到 `375x812` 检查窄屏布局：导航收缩为顶部图标栏，连接页和新增连接表单按单列排列；页面 `documentScrollWidth` 为 `366px`，未产生横向溢出。
- 检查完成后已调用视口重置并重新加载页面，恢复默认浏览器尺寸。

## Task 11：RedisInsight 视觉与排版对齐（2026-08-18）

### 参考项目视觉基线

- RedisInsight 的桌面壳层采用 `60px` 左侧导航栏，主区域使用整屏高度和内部工作区滚动，而不是居中窄内容卡片。
- 参考项目的核心间距以 `8px/16px/24px/32px` 为阶梯，页面/面板通常使用 `16px` 内边距和 `6px` 中等圆角。
- 当前参考分支深色主题的主要颜色为：页面背景 `#121212`、导航背景 `#0f1633`、面板背景 `#202020`、边框 `#383838`、主色 `#8ba2ff`、正文 `#dfe5ef`、次要文字 `#b5b6c0`；浅色主题使用白色面板、蓝色主色和 `#e4eaf2` 附近分隔线。
- RedisInsight 使用 Graphik/SourceCodePro/Inconsolata 组合；当前项目没有对应字体资源，因此保留系统无衬线字体和 JetBrains Mono 代码字体，重点复用其字重、字号和密度，不新增外部字体下载。
- Browser 结构重点是左侧键列表 + 右侧详情；Workbench 结构重点是上方命令编辑器 + 下方结果区域，二者均使用面板内滚动和细边界。

### 当前项目差异

- 当前 `src/App.tsx` 将标题、顶部标签、连接管理、Browser/Workbench 纵向堆叠在同一内容流，连接页在有连接后仍然占据主区域；这与参考项目的固定导航/上下文标题/单一工作区不一致。
- 当前 `src/styles.css` 使用绿色主色和较大的居中容器、10px 圆角、`clamp` 大留白；需要改为 RedisInsight 风格的蓝紫主色、深色中性层级、紧凑间距和全高工作台。
- 当前 Browser/Workbench 已具备正确的业务交互和测试覆盖，可优先通过 CSS 和轻量应用壳层调整完成视觉对齐，避免修改 Rust IPC 和数据行为。
- 现有工作区有用户未提交的 `src-tauri/` 变更；本轮不触碰这些文件。

### UI 设计工具基线

- `ui-ux-pro-max` 的生成结果建议开发者工具使用 JetBrains Mono + IBM Plex Sans、语义颜色 token、150–300ms 交互和 375/768/1024/1440 响应式检查。
- 该工具的通用颜色推荐与 RedisInsight 参考项目存在差异；本轮以用户指定的 RedisInsight 源码配色为主，保留当前项目的字体回退和可访问性要求。

### 已确认并实施的方向

- 应用壳层改为：左侧固定窄导航（连接/Browser/Workbench）+ 顶部连接上下文栏 + 右侧工作区；无活动连接时导航项保持禁用。
- 连接页改为 RedisInsight 风格的密集面板和空状态，仍保留当前保存/测试/打开/删除行为。
- Browser 改为更高密度的双栏全高布局，列表与详情分别滚动；Workbench 改为垂直编辑器/结果工作区，保留命令历史和结果展示。
- 统一浅色/深色 token、按钮/输入框/反馈状态、焦点环、减少动效和窄屏断点；不增加新业务能力、不引入第三方图标或云功能。

### 实施结果

- `src/App.tsx` 现在以单一应用壳层承载连接管理、Browser 和 Workbench；打开连接后只渲染选中的数据工作区，避免连接页与工作区纵向堆叠。
- `src/styles.css` 已重建为 RedisInsight 参考的深色优先蓝紫色板，并保留浅色主题、系统主题兼容、键盘焦点和 reduced-motion 规则。
- 前端测试已扩展到 56 项；生产构建和差异空白检查均已执行。

## Task 12：非 Cloud 功能差异勘察（2026-08-18）

### 当前 Redix 已有能力

- 连接管理：本地 Redis Standalone 的 host、port、username、password、database，连接配置 JSON 持久化，密码交给系统钥匙串保存。
- Browser：使用 `SCAN` 分页和 pattern 过滤，展示 key/type/TTL/size；支持 String、Hash、List、Set、Sorted Set 的整值读取、保存、删除和 TTL 修改。
- Workbench：当前连接上执行单条命令，支持引号/转义 tokenizer、结构化结果、错误映射、历史回填和 Cmd/Ctrl+Enter。
- 壳层：React/Vite + Tauri 2，连接管理、Browser、Workbench 三个工作区，深浅主题和 RedisInsight 风格排版。

### RedisInsight 非 Cloud 能力差异

参考项目 `/Users/ushopal/workspace/myself/RedisInsight` 的非 Cloud 页面、API module 和 controller 清单显示，当前缺失能力可分为以下独立批次：

| 批次 | 缺失能力 | 目标项目证据 | 当前 Redix 状态 | 备注 |
|---|---|---|---|---|
| A | Browser 完整数据类型 | `api/src/modules/browser/{stream,rejson-rl,vector-set,array,redisearch}` 及 `hash/list/set/string/z-set` controllers | 仅 5 类基础类型 | 优先补齐 Streams、JSON；Vector/Array/Search 依赖 Redis 模块，需独立处理能力探测 |
| B | Browser 生产力 | `pages/browser/components/{bulk-actions,add-key,browser-search-panel}`、browser-history、keys metadata/info | 无新增键、重命名、批量删除、导入导出、历史和信息面板 | 可在现有 SCAN/CRUD 之上增量实现 |
| C | Workbench/CLI | `pages/workbench`、`components/cli`、`components/command-helper`、`api/src/modules/{workbench,cli,commands}` | 单条命令、内存历史 | 可先做多命令/脚本执行、命令补全、结果复制/下载，再考虑 Monaco |
| D | 运维观察 | `pages/{slow-log,pub-sub}`、`components/monitor`、`api/src/modules/{slow-log,pub-sub,profiler}` | 无 | 需要长连接/流式事件和停止/清理生命周期，不能只靠现有 request-response IPC |
| E | 数据库概览与分析 | `pages/{database-analysis,instance,home}`、`api/src/modules/{database,database-info,database-analysis,database-recommendation}` | 无服务器信息、数据库切换、分析/推荐 | 适合在连接模型支持多 DB 后实现 |
| F | 连接拓扑与安全 | `pages/{autodiscover-sentinel,redis-cluster}`、`api/src/modules/{redis-cluster,redis-sentinel,ssh,certificate}` | 仅 Standalone TCP，无 TLS/SSH/Cluster/Sentinel | 需要扩展连接配置、Rust client 生命周期和拓扑路由，风险最高 |
| G | 本地设置与资源 | `pages/settings`、`api/src/modules/{settings,query-library,plugin}` | 仅内存页面状态，未持久化设置/查询 | Query Library 可先做本地 JSON 存储；插件和 Cloud/AI 不纳入范围 |

### 明确排除

- `api/src/modules/cloud`、`pages/autodiscover-cloud`、Azure 登录/发现、OAuth 云账户、云端 CAPI/job/subscription 等不移植。
- AI/Copilot、Telemetry/Analytics、远程插件市场不作为本轮本地 Redis 功能；若未来需要，必须另行确认数据和网络边界。
- 任何实现仍禁止在循环遍历中查询 SQL；当前架构使用 JSON/钥匙串和 Redis 命令，不引入 SQL 查询循环。

### Scope decision

用户已确认先执行 Browser 增强与 Stream/JSON 支持。首批以 docs/superpowers/specs/2026-08-18-browser-capability-expansion-design.md 为设计基线，以 docs/superpowers/plans/2026-08-18-browser-capability-expansion.md 为执行清单；后续仍按 A → B → C → E → D → F → G 分批，不把长连接运维和拓扑支持混入本批次。

### Task 12 交付验证补充

- Browser 首批能力已落地并通过前端、Rust、非 Cloud 静态检查和本机 Redis 集成验证。
- Stream 读取上限为 500 条；当前不包含 Consumer Group、实时订阅及其他模块专用编辑器。
- RedisJSON 采用根文档 `JSON.GET/JSON.SET`；普通 Redis 无 RedisJSON 模块时返回稳定的 `UNSUPPORTED_DATA_TYPE`。
- 本机 Redis 集成测试最初在沙箱内无法访问 `127.0.0.1`，使用授权的沙箱外测试命令重跑后通过；该环境限制不影响实现验证。

## Task 13：RedisInsight 非 Cloud 差异补全启动（2026-08-18）

### 当前状态

- 当前仓库工作区干净，分支为 `main`；本轮默认继续在当前分支工作，不创建新分支。
- 当前 Redix 已完成连接管理、Standalone Redis 数据访问、Browser 首批五类数据结构、Stream/RedisJSON 根文档、新增/重命名/批量删除/元数据刷新、Workbench 单条命令、深浅主题和 RedisInsight 风格应用壳层。
- 本轮目标从“单个 Browser 扩展批次”升级为“按目标项目差异继续补齐非 Cloud 功能”，需要重新确认分批范围，不能直接把完整 RedisInsight 一次性移植。

### 用户约束

- 对照 `/Users/ushopal/workspace/myself/RedisInsight` 实现缺失功能。
- Redis Cloud 相关功能排除；同时沿用既有排除边界：Azure 云接入、云账户/OAuth、AI/Copilot、Telemetry/Analytics、远程插件市场不进入默认实现。
- 默认在当前分支修改；提交信息使用简体中文。
- 禁止在循环遍历中查询 SQL；当前架构不引入 SQL 查询循环。

### 目标项目证据

- RedisInsight 当前 UI 页面至少包含：`browser`、`workbench`、`analytics`、`database-analysis`、`home`、`instance`、`slow-log`、`pub-sub`、`settings`、`vector-search`、`redis-stack`，另有 `autodiscover-cloud`、`autodiscover-azure`、RDI 等不在本轮范围内。
- Browser 目录除已有基础键浏览外，还包含键类型过滤、树视图、列配置、批量删除/上传、搜索面板、导入导出、数组、Vector Set、RedisJSON、Stream、Search 索引相关 UI；当前 Redix 仅覆盖分页键列表、基础编辑器、Stream/JSON 根文档和已落地的新增/重命名/批量删除/元数据刷新。
- Workbench/CLI 入口包含命令历史持久化、CLI 客户端生命周期、命令自动补全/帮助、raw/text/UTF-8 输出格式化、复杂结果摘要和可视化插件入口；当前 Redix 只有单输入框、单次 request-response 执行和当前会话内存历史。
- 目标 API 的本地 Redis 能力覆盖 `database`、`database-analysis`、`database-info`、`server`、`recommendation`、`query-library`、`settings`、`slow-log`、`pub-sub`、`profiler`、`cli`、`commands`、`workbench`，并支持 Redis Sentinel、SSH 隧道和证书；当前 Redix Rust 端仅有连接、Browser、Workbench 三组命令。
- `slow-log` 提供读取、清空和配置接口；`pub-sub` 与 `profiler` 使用 WebSocket gateway/订阅模型，说明它们不能只通过现有一次性 Tauri `invoke` 封装实现，需要新增可取消的长连接/事件桥接设计。
- 目标项目有数据库列表/连接/测试/导入导出、服务器信息、数据库概览与分析扫描、推荐等能力；当前 profile 虽有 `database` 字段，但没有数据库切换、实例信息、分析或推荐工作区。
- 目标项目 Workbench 的 `commands` API 为命令定义/补全数据源，`cli` API 负责独立 CLI 客户端和输出格式化；当前单次命令执行未提供命令定义、补全、脚本批量执行、raw 模式或持久化历史。
- 目标项目 `query-library` 与 `settings` 使用本地持久化仓储；当前 Redix 只有连接配置 JSON + 系统钥匙串，没有查询收藏或应用设置存储。
- 本次只读扫描发现目标项目没有独立的 `api/src/modules/redis-cluster` 目录，Cluster 实现分布在 `redis` client/connection 目录；连接差异不能仅靠模块目录名判断。

### 初步差异分层

1. 高收益、可复用现有架构：Browser 生产力补齐（搜索/类型过滤/导入导出/更丰富元数据）、Workbench 命令补全与多命令/格式化、数据库/实例概览、Query Library、设置持久化。
2. 需要新增 Rust/前端生命周期但仍属本地 Redis：Slow Log、Pub/Sub、Profiler；其中 Pub/Sub/Profiler 需要事件流与取消协议。
3. 依赖拓扑或安全连接重构：TLS、证书、SSH、Sentinel、Cluster、多数据库管理。
4. 可单独开关、依赖 Redis 模块或大型 UI：Vector Set、Array、Search/Query 索引、复杂结果可视化、插件系统；不应与基础批次混做。
5. 明确排除：Redis Cloud、Azure 云接入、云账户/OAuth、云端 API/发现、AI/Copilot、Telemetry/Analytics、远程插件市场。

### 设计约束

- 现有 Tauri IPC 是同步请求/响应的 typed wrapper；长连接能力需要独立设计事件通道、资源清理和连接切换竞态处理。
- 继续使用 Redis `SCAN` 分页，不引入阻塞式全量键枚举；当前任务不添加 SQL 查询，更不能在循环遍历中查询 SQL。
- 目标项目的 Electron/NestJS/Redux/Monaco 代码只能作为功能证据和交互参考，不直接复制运行时依赖；Redix 继续保持 Rust + Tauri + React 的轻量边界。

### 用户已确认的总体架构

- 采用方案 1：在现有 React + Vite + Tauri 2 + Rust 上按批次增量扩展 typed IPC、领域模型、状态和页面。
- 第一批按 `Browser 生产力 → Workbench 增强 → 数据库/实例概览 → Query Library/设置` 执行。
- Rust 按 `connections / browser / database / workbench / settings / observability` 组织命令；JSON 文件和系统钥匙串负责本地资源。
- 长连接能力采用单独的 Tauri event channel；拓扑与安全连接能力独立于第一批实现。
- 第一批数据流已确认：React 页面 → typed Tauri wrapper → Tauri command → Rust domain/service → 当前 Redis 连接或本地 JSON/钥匙串 → 固定错误/DTO → React 状态。
- Browser 首批增强保持 SCAN 分页；Workbench 增加内置命令目录、多命令执行、raw/text/JSON 展示、复制和按连接持久化历史；数据库概览集中返回只读聚合 DTO；Query Library/设置使用版本化 JSON 与原子替换。
- 错误策略已确认：固定错误码 `CONNECTION_NOT_OPEN`、`INVALID_INPUT`、`KEY_NOT_FOUND`、`UNSUPPORTED_DATA_TYPE`、`COMMAND_FAILED`、`PERSISTENCE_FAILED`、`OPERATION_CANCELLED`，不向前端暴露 Redis 底层错误文本。
- 兼容与安全策略已确认：使用 `COMMAND INFO`/`MODULE LIST` 探测能力；模块不可用时局部降级；密码只存钥匙串；敏感命令不写历史；本地 JSON 版本化并原子替换；连接切换和卸载取消旧请求。

### 实现计划文件地图

- Rust 应用入口和 Tauri 注册集中在 `src-tauri/src/lib.rs`；领域模型通过 `src-tauri/src/domain/mod.rs` 统一 re-export，命令通过 `src-tauri/src/commands/mod.rs` 注册。
- Browser 现有 Rust 边界是 `src-tauri/src/domain/key.rs`、`src-tauri/src/redis/connection_manager.rs`、`src-tauri/src/commands/browser.rs`；前端边界是 `src/features/browser/browserState.ts`、`BrowserPage.tsx`、`KeyList.tsx`、`KeyDetails.tsx`、`src/lib/types.ts` 和 `src/lib/tauri.ts`。
- Workbench 现有 Rust 边界是 `src-tauri/src/domain/workbench.rs`、`src-tauri/src/redis/workbench.rs`、`src-tauri/src/commands/workbench.rs`；前端边界是 `src/features/workbench/workbenchState.ts`、`WorkbenchPage.tsx`、`CommandInput.tsx`、`CommandResult.tsx`。
- 本地 JSON 原子写入模式已在 `src-tauri/src/persistence/profile_store.rs` 实现，可抽象为 Query Library/Settings 共用的版本化文档仓储；`src-tauri/tests/persistence.rs` 已有临时目录和原子写入测试工具。
- 应用导航位于 `src/App.tsx`，应用级回归位于 `src/app.smoke.test.tsx`；新增 Database/Settings/Query Library 页面必须在这里接入并覆盖未连接禁用状态。
- 现有 Tauri 注册列表在 `src-tauri/src/lib.rs:44-61`，新增 command 计划必须同步更新 Rust `commands` 模块、`generate_handler!`、前端 bridge 测试和 integration command adapter test。

### 计划自审结论（2026-08-19）

- 四个子计划已按 Browser → Workbench → Database → Query Library/Settings 拆分，依赖关系和总体验收矩阵一致。
- Workbench 历史、Query Library 和 Settings 均指向共享 `JsonDocumentStore`；未保留 `localStorage` 方案，损坏 JSON 恢复默认值时不覆盖原文件。
- 每个任务均包含明确文件、RED/GREEN 命令、实现边界和简体中文提交信息；第一批不引入 Redis Cloud、Azure、AI、Telemetry、远程插件或 SQL。

### Task 13 第一批交付复核（2026-08-24）

- Database/实例概览批次已在当前工作区落地并接入应用导航；只读聚合指标和数据库切换不向前端暴露 Redis 原始错误。
- Query Library 使用 `query-library.json`，Settings 使用 `settings.json`；两者均复用 `JsonDocumentStore`、版本字段和原子替换，损坏或未知版本文件恢复内存默认值且不覆盖原文件。
- Query Library 保存前复用 Workbench 的敏感命令规则，拒绝 AUTH、HELLO、ACL、CONFIG 命令族；回填 Workbench 是显式状态传递，不会隐式执行命令。
- Settings 的 `theme`、`result_format`、`scan_count`、`continue_on_error` 同时由 Rust/前端校验；根节点主题、Browser SCAN COUNT 和 Workbench 默认显示/批量策略均已联动。
- 交付前验证结果：前端 97/97、Rust 63 个已执行测试通过，Redis 集成测试 2 个按约定 ignored，非 Cloud 扫描、格式检查、构建和差异检查通过。
- 仍待后续独立批次评估的本地 Redis 能力包括 Slow Log、Pub/Sub、Profiler、TLS/SSH、Sentinel/Cluster 和 Vector/Array/Search 等模块专用功能；本轮未将其伪装成已完成。

## Task 14：当前项目与目标项目新一轮差异盘点（2026-08-24）

- 当前 `redix` 与目标 `RedisInsight` 的 Git 工作区均干净；当前分支是 `main`，本轮不创建新分支。
- 目标仓库的非 Cloud 页面/模块实际包括 `browser`、`workbench`、`analytics`、`database-analysis`、`instance`、`slow-log`、`pub-sub`、`settings`、`vector-search`、`redis-stack`，以及 `redis-sentinel`、SSH、证书等连接能力；Cloud、Azure、RDI、AI 和 Telemetry 需继续排除。
- 目标 UI/ API 证据显示，Browser 仍有 Array、Vector Set、RedisSearch/Query、树视图、批量动作和模块能力探测；当前 Redix 已覆盖基础五类型、Stream/JSON 根文档、批量删除、导入导出、类型过滤和元数据，但没有模块专用编辑器/索引工作区。
- 目标 `slow-log` 支持读取、清空、配置；`pub-sub` 和 `profiler` 是长连接/订阅模型，不能直接套用当前一次性 Tauri `invoke`，需要事件通道、取消和连接切换清理。
- 目标 Workbench/CLI 还包含独立 CLI 生命周期、命令定义/补全、Raw/Text 输出格式化、复杂结果处理；当前 Redix 已有本地命令目录、多命令执行、Raw/Text/JSON 结果和持久化安全历史，仍缺少 CLI 流式/监控模式和复杂可视化。
- 目标 API 还覆盖数据库分析、推荐、连接导入导出、多数据库管理、TLS/证书、SSH、Sentinel/Cluster；当前 Redix 只支持 Standalone TCP，已有只读实例/数据库概览和数据库切换。
- 对当前架构而言，下一批最适合独立交付的是“本地 Redis 运维观察”：Slow Log request-response + Pub/Sub 可取消事件流；Profiler 可在同一事件协议稳定后再实现。TLS/SSH/Sentinel/Cluster 和模块专用能力应另立设计，不能与运维观察批次混杂。

### Task 14 精确接口盘点

- 目标 Slow Log 的数据模型为 `id`、Unix 秒级 `time`、微秒 `durationUs`、命令参数数组拼接后的 `args`、客户端地址 `source` 和可选客户端名 `client`。
- 目标 Slow Log 的本地 Redis 操作对应 `SLOWLOG GET count`、`SLOWLOG RESET`、`CONFIG GET slowlog-*` 和 `CONFIG SET slowlog-log-slower-than/slowlog-max-len`；当前 Redix 可先限制为 Standalone，读取、清空、读取配置和更新配置均为 request-response。
- 目标 Pub/Sub 支持普通 channel 与 pattern channel 订阅、取消订阅、发布消息，并通过独立 subscriber client 接收 `{channel, message, timestamp}`；目标实现还区分 subscriber 连接和发布连接。
- 当前 Tauri 依赖已经包含 `@tauri-apps/api`，前端可使用 `listen`/`emit` 事件 API；Rust 端可用 `tauri::AppHandle::emit` 将后台订阅任务的消息发送到前端，但需要在 `AppState` 中保存按连接/订阅会话的取消句柄，并在停止、断开和连接切换时清理。
- 本批设计将 Pub/Sub 事件限制为普通 Standalone TCP、最多一个前端会话对应一个订阅任务、最大缓存消息数 5000；超出上限丢弃最旧消息并发出稳定的 overflow 事件，不把 Redis 底层错误文本传到前端。
- 本批不实现 Profiler、Cluster/Sentinel fan-out、TLS/SSH、Streams Consumer Group、Redis Cloud/Azure、Telemetry/AI/远程插件；这些能力保留在差异清单中另立批次。

### Task 14 实施计划补充盘点

- 当前前端 bridge 统一通过 `call<T>` 包装 `@tauri-apps/api/core` 的 `invoke`，新增 Slow Log 的 4 个 request-response wrapper 可直接复用；Pub/Sub listener 需要额外接入 `@tauri-apps/api/event` 的 `listen`/`UnlistenFn`。
- 当前 AppState 只持有 `RedisService`、profile/secret repository 和 data_dir；Pub/Sub 生命周期应放在 `RedisService` 内部或其独立 manager 中，避免把 Redis Client 暴露到 command/UI 层。
- 当前 Rust `Cargo.lock` 已包含 `futures-util 0.3.33`，新增 direct dependency 可离线解析；`redis 1.5.0` 提供 `Client::get_async_pubsub`、`PubSub::subscribe`/`psubscribe` 和 `on_message`。
- 当前前端脚本为 `npm run test:frontend`、`npm run build`、`npm run check:non-cloud`；Rust 使用 `cargo test --manifest-path src-tauri/Cargo.toml` 和 `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check`。
- 规划读取时一次 JS 编排命令因字符串转义产生 SyntaxError，未触碰文件；随后拆分为简单字符串重跑成功。

### Task 14 运维观察交付复核（2026-08-24）

- Slow Log service 使用 `SLOWLOG GET/RESET` 和 `CONFIG GET slowlog-*`；配置 parser 同时覆盖 RESP2 pair array、RESP3 Map 和 Attribute 包装，前端不显示底层 Redis 错误文本。
- Pub/Sub 使用 `Client::get_async_pubsub` 独立 socket，不复用业务 multiplexed connection；每个 connection id 只有一个 `JoinHandle`，新会话、关闭连接、打开替换连接和数据库切换均会 abort 旧任务。
- Tauri 事件固定为 `redix://pubsub/message` 与 `redix://pubsub/status`；前端 listener 按 connection/session 过滤，组件卸载会解除 listener 并停止当前会话，消息缓存最多 5000 条。
- 新增“运维观察”导航与 Slow Log/Pub/Sub 页面，沿用现有 RedisInsight 风格 token、响应式表格、可见焦点和 reduced-motion 规则；未新增云入口、模块编辑器或 SQL。
- 安全验证：`cargo test`、`npm test`、`npm run build`、`npm run check:non-cloud`、`cargo fmt --check` 均通过；真实集成用例包含 `SLOWLOG RESET`，因破坏性副作用未在沙箱外执行。
