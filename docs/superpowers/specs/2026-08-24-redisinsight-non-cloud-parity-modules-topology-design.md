# RedisInsight 非 Cloud 功能差异补全设计

## 状态

- 日期：2026-08-24
- 参考项目：`/Users/ushopal/workspace/myself/RedisInsight`
- 实现项目：`/Users/ushopal/workspace/myself/redix`
- 状态：已获得用户对总体方案的确认，进入实现计划阶段前的书面设计审阅

## 背景

Redix 已经完成本地 Standalone Redis 的连接管理、Browser 基础数据类型、Stream Consumer Group、Database/Instance、Database Analysis、Slow Log、Pub/Sub、Profiler、Workbench 基础工作区、Query Library、Settings、连接配置导入导出和 TLS。与参考项目相比，当前缺口集中在四类：

1. Redis 模块能力：RedisJSON 深层路径编辑、RedisSearch/Query、Vector Set、Array 以及其他模块类型的能力探测和可用工作区。
2. Workbench/CLI 高级体验：模块命令提示、结构化结果、复杂结果查看和更完整的本地 CLI 工作区。
3. 连接拓扑：SSH 隧道、Sentinel 自动发现/主节点连接、Cluster 种子发现、拓扑详情和命令路由。
4. 文档和边界一致性：当前 README 仍把 TLS 描述为未支持，需要与已经交付的代码同步。

这些能力不能通过在现有 Standalone 页面中继续堆叠按钮完成。模块能力依赖服务器模块和命令版本，拓扑能力会改变 profile、连接生命周期、命令路由、错误模型以及 Pub/Sub/Profiler/Analysis 的行为，因此采用可独立验收的垂直切片。

## 目标

- 对齐 RedisInsight 中可以归属于本地 Redis 的高频能力，并保留 Redix 的 Rust + Tauri + React 架构。
- 让模块和拓扑能力在不可用时安全降级，不因一次 `MODULE LIST` 或单个命令失败而破坏现有 Standalone 功能。
- 通过 typed IPC、固定应用错误码和输入/响应上限，保证前端不直接操作 Redis client，也不把底层错误文本作为产品协议。
- 在每个切片中保留真实 Redis/Redis Stack 集成测试入口，但默认 ignored，未配置外部实例时不把模拟测试当成网络验收。

## 非目标与硬边界

- 不实现 Redis Cloud、Azure Managed Redis、RDI、云登录、云账户、云端点、云数据库发现或云 SDK。
- 不实现 AI/Copilot、Telemetry、远程插件运行时或插件市场。
- 不引入 SQL、TypeORM 或其他 SQL 持久化；任何遍历中不得查询 SQL。
- 不使用 `KEYS` 作为 Browser、分析或批量操作的实现路径。
- 不直接复制 RedisInsight 的 Electron、NestJS、Redux、Monaco 或插件代码；只参考其用户流程和 Redis 命令协议。
- 不在模块不可用时伪造数据类型；UI 必须显示稳定的“不支持的数据类型/模块不可用”状态。

## 差异范围和交付顺序

### 第一批：模块能力基础与 RedisJSON 深层编辑

建立连接级 capability snapshot，提供模块名称、版本和已确认命令族；连接打开、关闭或替换时清理快照。探测结果只作为 UI 能力提示，真正执行前仍由 Redis 返回结果决定。

在 Browser 的 JSON 详情中补齐路径级读取、设置、删除和数组/对象的基本编辑。保留现有根文档编辑兼容行为，路径输入使用受限 JSONPath/RedisJSON path 字符串，不执行本地表达式，不把路径拼接为 SQL 或 shell。

第一批验收：

- Standalone Redis 未安装 RedisJSON 时，普通五类数据和现有 JSON 降级行为不回归。
- Redis Stack 有 JSON 时，可读取根路径和嵌套路径，设置/删除嵌套值，并在键切换或连接切换后丢弃过期响应。
- RESP2 数组、RESP3 map/attribute 和空结果均有解析测试；坏数据只映射为固定错误。

### 第二批：RedisSearch/Query 工作区

新增 capability-gated 的 Search/Query 工作区，覆盖索引列表、索引信息、查询和有限分页结果；Browser 对 Search 索引键提供跳转入口。命令使用 `FT.*` 协议，查询文本作为用户输入传入 Redis，不由前端拼接未转义的命令字符串。

第一版不做后台索引同步、远程推荐、AI 查询生成或跨节点 fan-out；索引详情和查询结果采用固定大小上限，结果可以在 Workbench 中回填为普通命令。

### 第三批：Vector Set 与 Array

在能力探测确认服务器支持对应命令族后，补齐 Vector Set 的创建/添加/属性/相似度查询/删除，以及 Array 的浏览、范围读取、搜索/聚合、元素编辑和删除等高频流程。每个面板都以连接和键为作用域，不引入全局后台扫描。

向量输入校验维度、数值和 top-k 上限；Array 的范围、搜索和聚合均限制返回条数和响应大小。模块不存在、命令不存在或版本不兼容时显示可恢复的降级状态。

### 第四批：Workbench/CLI 高级体验

复用现有 Workbench 的命令执行和安全历史边界，补充模块命令目录、按命令族的参数提示、Raw/Text/JSON 结果切换和较复杂嵌套结果的折叠查看。CLI 使用同一连接和 typed IPC，不单独引入 Node 后端或远程执行环境。

不引入 Monaco 作为第一版硬依赖；现有编辑器能够承载的输入和提示优先落地，只有在测试和 bundle 预算允许时再评估更重的编辑器依赖。

### 第五批：SSH、Sentinel、Cluster 拓扑

拓扑批次在模块批次完成后单独实现，并在实现前对连接 backend 和依赖做再次技术审查。profile 采用版本化连接类型，连接 manager 通过统一的连接 handle/路由接口暴露 Standalone、SSH 隧道、Sentinel 主节点和 Cluster 节点；禁止把拓扑连接伪装成一个普通 Standalone host/port。

- SSH：保存 host、port、user、认证方式和密钥引用；私钥只进系统安全存储，隧道关闭时清理后台任务和本地监听端口。
- Sentinel：保存 Sentinel 种子、master name、认证信息和 TLS/SSH 组合约束；连接时解析当前 master，切换/故障时清理旧 client。
- Cluster：保存种子节点，发现 slot/node 信息并按 key 路由命令；只读详情和可安全 fan-out 的观察功能必须明确节点范围，不能假设所有节点共享同一个数据库。

拓扑批次会给 Pub/Sub、Profiler、Slow Log、Database Analysis 和批量删除增加“当前节点/全部节点”边界；在没有明确路由语义前，不自动复用 Standalone 的全库操作。

## 架构设计

### Rust domain 与 capability

新增独立的模块能力领域模型，至少包含：

- 模块名称、版本和标准化命令族；
- 当前连接的 capability 状态、探测时间和失败降级原因；
- JSON path、Search query、vector 参数、array 范围等输入的验证结果；
- 模块动作的稳定 DTO，避免把 `redis::Value` 或任意 serde value 直接暴露到前端。

Capability snapshot 只存在于连接 session 内，不写入 `connections.json`，也不包含密码、证书、SSH 私钥或原始 Redis 错误。未知模块保留为不可启用状态，不根据模块名猜测可执行命令。

### Redis service 与 typed IPC

现有 `RedisService` 继续负责 client 生命周期、数据库绑定、错误映射和独立 socket。模块操作按领域拆到独立 service 文件，由 Tauri command 接收已校验的 connection id、key 和 DTO，再调用 service；React 只通过 `src/lib/tauri.ts` 的 typed wrapper 调用 command。

所有 request-response command 都遵守：

- 连接/键/路径/查询字符串有长度上限；
- 返回条数、向量 top-k、Array range 和 JSON payload 有上限；
- Redis 命令失败统一映射为现有固定错误码或新增稳定错误码，不向 UI 泄露连接地址、认证内容或原始服务器堆栈；
- 连接、键或 session token 发生变化时，前端只接受最新请求结果。

### RESP2/RESP3 兼容

模块 parser 需要覆盖 RedisInsight 当前支持的常见 RESP2 数组、RESP3 map、attribute 包装、空值和错误值；parser 测试直接使用脱离网络的 `redis::Value` 样本。对未知字段采用可忽略策略，对必需字段缺失返回固定解析错误。

### 前端工作区

Browser 的键详情继续负责具体 key/value 编辑；Search/Query、Vector Search 和拓扑详情使用独立 feature/page，避免把所有模块状态塞进 `browserState`。页面状态按 connection id、database、key 和 request token 绑定，卸载时解除事件监听或取消可取消任务。

模块面板必须具备三种状态：能力可用、能力未知/正在探测、能力不可用。能力不可用时仍允许打开普通 Browser、Workbench 和 Database 页面。

## 安全、性能和数据边界

- Profile JSON 只保存连接元数据和 capability-independent 配置；密码、TLS 材料、SSH 私钥继续使用现有安全存储。
- 第一版固定限制 JSON path 512 字节、JSON payload 5 MiB、Search/Vector 单页 200 条、Array 单次返回 500 条、查询文本 4096 字节，完整 Redis 响应 4 MiB；这些常量必须在领域测试中固定。
- Search、Vector、Array 和 JSON 只使用显式命令，不执行用户提供的 shell、SQL 或本地脚本。
- 模块探测失败不得阻塞连接打开；功能动作失败也不得清理或覆盖用户当前 key，除非 Redis 已确认写入成功。
- 批量操作继续使用 SCAN 和分批命令；拓扑批次在确定路由前不承诺跨节点批处理。

## 测试策略

每个垂直切片遵守 RED → GREEN → REFACTOR：

1. 先为一个可观察行为写 domain/parser/command 或 React 页面失败测试，并确认失败原因是缺少功能。
2. 用最小实现通过定向测试，再补错误、空结果、能力不可用、连接切换和过期响应场景。
3. 完成后运行现有全量 Rust/前端回归、生产构建、非 Cloud 扫描、格式检查和 diff 检查。
4. Redis/Redis Stack 真实流程使用环境变量开启，默认 ignored；没有环境变量时只报告未执行，不虚报通过。

至少覆盖：

- capability snapshot 的正常、未知模块、探测错误和重新连接清理；
- JSON path 读取/写入/删除、非法 JSON、空结果、超限和竞态响应；
- Search index/query 的 RESP2/RESP3、空索引、无结果、分页和命令错误；
- Vector/Array 的参数验证、能力降级、结果上限和写入后刷新；
- Workbench 模块命令提示和敏感命令历史过滤；
- 拓扑批次的 profile 迁移、旧 client 清理、错误连接回滚和节点路由。

## 交付与文档

- 每个垂直切片使用简体中文提交信息，提交前运行对应的完整验证矩阵。
- 每批完成后更新 `task_plan.md`、`findings.md`、`progress.md` 和 README 的能力/边界说明。
- README 必须把已支持的 Standalone TLS 写入能力清单，并把尚未实现的 SSH、Sentinel、Cluster、模块编辑器与 Cloud/AI/Telemetry 等明确区分。
- 非 Cloud 静态检查只扫描生产源码和 package scripts；设计文档中的排除项不应被误判为产品入口。

## 验收标准

本设计的总体目标在以下条件全部满足后才算完成：

- 目标项目列出的本地高频模块、Workbench/CLI 和拓扑能力，已经按批次实现或在 README/范围文档中明确标注未实现原因；
- 任何未安装模块、不可用拓扑和 Redis 版本差异都能稳定降级；
- 现有 Standalone 功能、TLS、密码/证书安全边界和本地持久化不回归；
- Rust、前端、构建、非 Cloud、格式和差异验证均有新鲜命令证据；
- 没有 Redis Cloud、Azure、RDI、AI、Telemetry、远程插件或 SQL 生产入口。
