# RedisInsight 本地功能全量对齐设计

## 1. 背景与目标

当前 Redix 已在 Rust + Tauri + React 架构上完成 Standalone/TLS、Sentinel、受限 SSH、Browser、Workbench、CLI、RedisJSON、Search、Array、Vector Set、Stream、Slow Log、Pub/Sub、Profiler、实例概览和数据库分析等多轮实现。本设计以 Redix `27ea136` 和只读参考项目 RedisInsight `48ee19fab` 为基线，继续补齐仍缺失的本地 Redis 功能。

目标是对齐 RedisInsight 的用户可见工作流、Redis 命令语义、安全边界和错误行为，不复制其 Electron、NestJS 或内部存储实现。

## 2. 范围

### 2.1 纳入范围

- Redis Standalone、Sentinel、Cluster 的本地连接与拓扑操作。
- macOS、Windows、Linux 上的 TLS、证书、SSH、文件导入导出和原生打包行为。
- Browser 的本地值解码、大规模批量任务、Stream 高级能力和剩余编辑动作。
- RedisSearch、Vector Search、Workbench/CLI 的高级本地工作流。
- 随应用发布、只读白名单加载的本地内置结果可视化。
- 后台数据库分析、版本化历史、本地推荐和任务生命周期。
- 连接标签、查询包、快捷键、本地通知和跨平台自动更新等本地产品能力。

### 2.2 永久排除

- Redis Cloud。
- Azure Managed Redis。
- RDI。
- AI/Copilot。
- Telemetry/Analytics。
- 远程插件、插件市场、远程插件下载和远程代码执行。
- 云登录、云账户、云资源发现、云 SDK 和云端点。

以上排除项不通过 feature flag 隐藏，而是不进入生产源码、依赖、菜单和网络访问路径。

### 2.3 不采用的实现方式

- 不引入 RedisInsight 的 Electron/NestJS 运行时兼容层。
- 不建立 Rust 与 Node 双后端。
- 不使用 `KEYS` 代替扫描。
- 不引入 SQL；任何后续实现均不得在循环遍历中查询 SQL。
- 不以单元测试替代真实 Redis/Redis Stack/拓扑环境验证。

## 3. 总体架构

现有四层结构继续保留并扩展：

1. **Rust 领域层**：定义版本化 DTO、输入约束、RESP2/RESP3 解析、响应上限和固定错误码。
2. **Redis 与平台服务层**：负责连接生命周期、拓扑路由、TLS/SSH、后台任务、安全存储和跨平台适配。
3. **Tauri typed IPC**：只暴露稳定、有限、可取消的命令和事件，不泄露凭据、原始 Redis 错误或平台内部路径。
4. **React 功能层**：负责页面状态、旧请求隔离、分页、能力降级、进度展示和无障碍交互。

不进行一次性连接层重写。每个批次只提取当前批次所需的最小公共接口，并用现有测试保护已交付的 Standalone、Sentinel、SSH 和模块能力。

## 4. 核心组件

### 4.1 统一连接目标

连接模型扩展为以下明确目标：

- `StandaloneTarget`
- `SentinelTarget`
- `ClusterTarget`

TLS、SSH、认证和数据库选择作为可组合的传输配置，但由领域校验定义允许组合。`ConnectionManager` 负责连接句柄、重连、拓扑刷新、代次隔离和清理。`NodeScope` 明确操作作用于逻辑数据库、当前主节点、指定节点或整个拓扑，禁止在 Cluster 下默认套用单节点语义。

Cluster 服务需要处理 MOVED/ASK、slot map 刷新、节点健康状态和受限重试。Browser SCAN、观察和分析必须明确采用单节点、主节点集合或全拓扑 fan-out，并对去重、部分失败和响应上限建立稳定合同。

### 4.2 能力注册表

`CapabilityRegistry` 统一根据以下信息判断能力：

- Redis 版本与协议模式。
- 已加载模块及模块版本。
- 连接拓扑和节点角色。
- 操作系统及可用平台能力。
- 当前连接是否启用 TLS、SSH 或特定认证方式。

能力不可用时只局部禁用对应功能，并向 UI 返回稳定原因；不能使普通连接、Browser 或 Workbench 整体失败。

### 4.3 后台任务管理器

`BackgroundTaskManager` 管理批量删除、导入、数据库分析和推荐等长任务。每个任务具有：

- 版本化输入和状态 DTO。
- 进度、成功数、失败数和有界失败摘要。
- 取消令牌、超时、应用退出清理和连接关闭清理。
- 可恢复元数据；敏感命令和凭据不得进入任务记录。
- 明确的部分成功语义，不能把部分失败报告为完成。

### 4.4 本地解码与可视化注册表

`DecoderRegistry` 只注册编译进应用的解码器，并在字节、深度、字段数和执行时间上设限。需要用户 schema 的格式只读取用户明确选择的本地文件或文本，不保存任意路径为后台隐式读取源。

`BuiltinVisualizationRegistry` 只加载随应用发布并列入静态白名单的本地实现。它接收结构化、有界 DTO，不获得网络、文件系统、凭据或任意命令执行权限。远程 manifest、下载、动态代码、插件市场和远程更新永久排除。

## 5. 统一数据流与错误边界

统一调用链为：

`React 输入 → 前端校验 → typed IPC → Rust 领域校验 → CapabilityRegistry → 连接或任务服务 → Redis/平台 API → 有界 DTO → React`

所有外部输入在 Rust 边界重新校验。字符串、数组、节点数、扫描页、分析键数、解码字节数、可视化行列和 IPC 响应均有硬上限。

新增或扩展稳定错误类别至少覆盖：

- 拓扑不可用、节点不可达、节点部分失败。
- MOVED/ASK 重定向耗尽、slot map 过期。
- 平台不支持、传输组合不支持。
- 解码格式无效、schema 无效、解码结果超限。
- 任务取消、任务超时、任务部分失败。
- 内置可视化不支持当前结果。

错误不得包含密码、连接 URI、私钥、证书正文、SSH 命令行、Redis 原始错误文本或本机敏感路径。

## 6. 用户体验

- 连接页支持 Standalone、Sentinel、Cluster，并按平台与组合能力动态显示 TLS、SSH、认证和证书选项。
- 数据库工作区显示拓扑与节点上下文；跨节点操作必须明确展示作用范围和部分失败。
- Browser 的解码器、批量任务、Stream 和模块动作继续嵌入键详情，保留现有分页与旧响应保护。
- Search/Vector 采用索引定义、查询构建、结果查看三段式流程。
- Workbench 提供原始值、树、表格和本地内置可视化；CLI 增强仍保持独立会话生命周期。
- 长任务进入统一任务中心，页面切换不丢进度，支持取消、重试和查看有界失败原因。
- 能力不可用时展示 Redis 版本、模块、平台或连接类型限制，不显示无法执行的假入口。

视觉继续沿用 Redix 当前设计系统，不追求 RedisInsight 像素级复制。

## 7. 实施批次

### 批次 1：连接与拓扑

- Redis Cluster profile、slot 路由、拓扑刷新和节点健康状态。
- 跨节点 Browser SCAN、实例观察和分析的作用域、去重和部分失败语义。
- Windows SSH 实现，以及 TLS/SSH/Sentinel 的合法组合补齐。
- 跨平台安全存储和连接导入导出迁移。

该批次优先执行，因为后续所有 Redis 操作都依赖稳定的连接目标和节点作用域。

### 批次 2：Browser 深化

- RedisInsight 支持的本地 String 值解码格式与 schema 工作流。
- 有进度、取消和部分失败报告的大规模批量任务。
- Stream 实时消费、XAUTOCLAIM 和剩余高级 Claim/编辑动作。
- 当前功能矩阵中仍为部分支持的 Browser 编辑细节。

### 批次 3：Search、Vector 与 Workbench

- 高级索引字段、向量索引和查询流程。
- 可视查询构建器及受限结果回填。
- Workbench/CLI 的协议、推送、命令帮助和结果体验深化。
- 只读白名单的本地内置结果可视化。

### 批次 4：分析与推荐

- 可取消的后台数据库分析。
- 版本化时间序列、相同参数比较和有界保留策略。
- 基于本地扫描和 Redis 指标的可解释推荐。
- 分析任务恢复、删除和部分失败处理。

### 批次 5：本地产品能力

- 连接标签和过滤。
- 完整查询包、导入导出和本地资源迁移。
- 快捷键、自定义本地通知和设置细节。
- macOS、Windows、Linux 的自动更新与回滚安全边界；不连接 Redis Cloud。

### 批次 6：跨平台收口

- macOS、Windows、Linux 编译与原生包冒烟。
- 平台特定 SSH、钥匙串/凭据管理、证书和文件行为验证。
- Tauri 桌面端主流程 E2E。
- 最终功能矩阵、范围扫描和文档一致性复核。

## 8. 测试策略

每个批次必须同时包含：

- Rust 单元测试：DTO、校验、RESP2/RESP3、路由、取消和错误映射。
- Redis 集成测试：隔离 Standalone、Sentinel、Cluster 和 Redis Stack；缺少环境时明确 skip。
- typed IPC 契约测试：Rust/TypeScript 字段、枚举、事件和错误结构一致。
- React 测试：主工作流、竞态、旧响应、局部降级、任务生命周期和可访问性。
- 安全测试：凭据不落盘、不进入日志/导出；排除入口和远程插件无法进入生产源码。
- GitHub Actions 平台矩阵：macOS、Windows、Linux 编译、测试和原生打包冒烟。

当前 macOS 机器只能声明实际完成的 macOS 验证。Windows/Linux 若只在 CI 验证，交付记录必须明确区分 CI 与实机结果。

## 9. 完成标准

一项功能只有同时满足以下条件才能在矩阵中标为支持：

1. 用户能够完成与 RedisInsight 等价的本地工作流。
2. Redis 命令语义、拓扑作用域和部分失败行为明确。
3. 安全与敏感数据边界不弱于参考项目。
4. 成功、失败、取消、竞态和能力降级路径有测试。
5. 文档和功能矩阵与实际实现一致。

最终完成时，功能矩阵中除永久排除项外不得存在“缺失”。因 Redis 版本、模块或操作系统限制无法提供的能力，必须有能力检测、UI 说明和测试，而不是静默缺失。

## 10. 约束与工作方式

- 默认在当前 `main` 分支直接修改，不擅自创建新分支。
- 参考项目只读，不修改 `/Users/ushopal/workspace/myself/RedisInsight`。
- 实现按 TDD 先写失败测试，再写最小实现。
- 每个批次独立验证、更新矩阵，并使用简体中文提交信息。
- 不自动 push、发布、签名、安装或连接未获授权的外部服务。
