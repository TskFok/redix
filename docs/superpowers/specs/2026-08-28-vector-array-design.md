# Redis Vector Set 与 Array 功能补全设计

## 文档状态

- 日期：2026-08-28
- 状态：已确认，待实施
- 对照项目：/Users/ushopal/workspace/myself/RedisInsight
- 当前项目：Redix
- 本批次：Browser 中的 Vector Set 与 Redis Array

## 1. 背景与目标

当前项目已经覆盖 Redis 基础数据类型、RedisJSON 根文档、RedisSearch 基础查询、Workbench、Database、Pub/Sub、Profiler 和 Slow Log 等能力，但 Browser 仍无法识别或操作 RedisInsight 已支持的 Vector Set 与 Redis Array。

本设计的目标是在保持当前“Rust 负责 Redis 访问、Tauri 负责受控 IPC、React 负责交互”的边界下，补齐以下非 Redis Cloud 功能：

1. 连接建立后识别当前 Redis 是否支持 Vector Set 和 Array，并将能力以稳定快照暴露给前端。
2. Browser 能够浏览、创建、编辑、搜索和删除 Array。
3. Browser 能够浏览、创建、编辑、查询和删除 Vector Set，并处理元素向量及属性。
4. 对 RESP2/RESP3 返回值、64 位索引、二进制向量、结果规模和模块不可用等情况做显式处理。
5. 不因新模块不可用而破坏现有连接、Browser 或其他 Redis 数据类型功能。

本设计只定义第一批功能的实现边界。RedisJSON 深层树浏览、SSH/Sentinel/Cluster、Workbench 高级 CLI、以及其他后续差异不在本批次实现。

## 2. 范围与非目标

### 2.1 本批次范围

- Standalone TCP/TLS 活跃连接。
- Redis Array：
  - 创建与 TTL；
  - 长度、计数、下一个可用索引；
  - 区间读取与扫描；
  - 单元素读取、批量读取；
  - 追加、按索引设置、按索引删除、按区间删除；
  - ARGREP 搜索；
  - AROP 聚合；
  - Browser 中的 key 详情、刷新和错误状态。
- Vector Set：
  - 创建；
  - 批量添加或更新元素；
  - 元素列表与总数；
  - 元素向量和属性读取；
  - 属性设置与删除；
  - 元素删除；
  - VSIM 相似度查询；
  - 向量以 FP32/base64 安全传输和下载；
  - Browser 中的 key 详情、刷新和错误状态。
- Rust domain 类型、Redis 命令构造、RESP2/RESP3 解析、Tauri 命令、TypeScript 类型、Browser UI 和测试。

### 2.2 明确不在本批次

- Redis Cloud、Azure Managed Redis、RDI、Redis AI/Copilot、遥测上报和远程插件运行时。
- SSH、Redis Sentinel、Redis Cluster、自动发现和拓扑监控。
- RedisJSON 深层对象/数组树浏览和下载。
- Workbench 的独立 CLI 会话、复杂结果可视化和完整命令帮助系统。
- SQL/SQL Editor。
- 任何需要 KEYS 的实现；key 浏览继续使用已有的 SCAN 流程。
- 用前端 JavaScript number 表示 Array 的 64 位索引。

## 3. 用户可见行为

### 3.1 能力不可用时

连接能力快照新增 array_supported 与 vector_set_supported。能力为 false 时：

- 连接仍然成功，已有功能不受影响；
- Browser 不显示可执行的模块操作按钮；
- 如果用户通过 SCAN 打开了对应类型的 key，显示“当前连接不支持该数据类型”，而不是显示空数据或将连接标记为失败；
- 通过命令直接调用对应 Tauri API 时返回结构化 UnsupportedFeature；
- 不通过模块名猜测能力，能力快照必须来自版本/命令能力探测的可验证结果。

### 3.2 Array 详情页

Array 详情页以“索引—值”表格为主，支持以下操作：

- 创建 Array 时输入 key、初始元素和可选 TTL；
- 显示 ARLEN、ARCOUNT 和 ARNEXT 的结果；
- 通过区间输入读取一段连续索引；
- 通过扫描模式只显示已填充的元素，同时保留索引；
- 单元格内编辑元素；
- 追加元素；
- 输入明确的十进制索引写入元素；
- 选择索引后批量删除，或按 [start, end] 删除；
- 输入搜索模式执行 ARGREP；
- 选择聚合操作执行 AROP，展示聚合结果；
- 刷新后保留当前 key 和合法的分页/区间参数。

区间读取和扫描必须区分“连续索引范围”和“已填充元素”。稀疏 Array 中的空洞不能在 UI 或 API 层被误当成不存在的 key，也不能丢失真实索引。

### 3.3 Vector Set 详情页

Vector Set 详情页以元素列表和详情面板为主，支持以下操作：

- 创建 Vector Set 时配置名称、向量维度、量化方式及可选初始元素；
- 显示总元素数以及服务端返回的向量配置；
- 分页列出元素；
- 查看单个元素的向量和属性；
- 添加或更新元素；
- 设置、更新、删除元素属性；
- 删除元素；
- 通过以下三种互斥输入之一执行 VSIM：
  - 元素名称；
  - 数字向量；
  - FP32 二进制的 base64 表示；
- 展示相似元素、距离/分数和可选属性；
- 对单个元素下载 FP32 向量。

相似度查询至少包含 key、查询来源、top-k 和是否返回属性。查询来源只能选择一种；空输入、维度不一致、非有限数字和非法 base64 在前端提交前以及 Rust domain 层再次校验。

## 4. 领域模型与边界

### 4.1 ModuleCapabilities

现有 ModuleCapabilities 增加：

    array_supported: boolean
    vector_set_supported: boolean

后端对应字段使用 Rust bool。现有 JSON、Search 字段保持兼容。能力探测产生一个完整快照并替换旧快照，不在不同命令中隐式重复探测。

能力探测的来源按以下优先级组合：

1. 解析 MODULE LIST 和 Redis server version；
2. 使用 COMMAND INFO 或等价的无副作用命令能力检查确认实际命令集合；
3. 只有在必需命令集合足够完整时才将对应能力标记为 true。

单个命令缺失不得让整个连接失败。探测过程中的非致命不支持错误转化为 false；网络、认证或协议错误继续使用现有连接错误路径。

### 4.2 RedisValue

RedisValue 增加轻量摘要变体，避免打开 Browser 列表时直接加载模块的大量内容：

    Array {
      length: string,
      count: string
    }

    VectorSet {
      total: string,
      dimension: optional number,
      quantization: optional string
    }

其中 Array 的 length、count 和所有索引均使用规范化十进制字符串；Vector Set 的总数也使用字符串，避免跨协议或未来服务端实现产生整数溢出。摘要不包含完整元素、属性或向量；详情必须通过专用 API 分页获取。

现有 KeyValue、基础 key 类型和已有编辑器行为保持兼容。未知 Redis 类型继续返回 UnsupportedDataType，不把未知类型强行映射成 Array 或 Vector Set。

### 4.3 Array domain 类型

Array domain 层定义以下稳定 DTO：

    ArraySummary {
      key: string,
      length: string,
      count: string,
      next_index: string
    }

    ArrayElement {
      index: string,
      value: string
    }

    ArrayRange {
      elements: Array<ArrayElement>,
      start: string,
      end: string,
      has_more: boolean
    }

    ArraySearchResult {
      elements: Array<ArrayElement>,
      total: string
    }

    ArrayAggregateResult {
      operation: string,
      value: string
    }

实际实现可为聚合结果补充服务端必要的数值/字符串类型字段，但前端不直接接收 redis::Value。所有写入接口使用明确的 key、索引、值和超时字段，不接受拼接好的任意命令字符串。

### 4.4 Vector Set domain 类型

Vector Set domain 层定义以下稳定 DTO：

    VectorSetSummary {
      key: string,
      total: string,
      dimension: optional number,
      quantization: optional string
    }

    VectorSetElement {
      name: string,
      score: optional number,
      vector_base64: optional string,
      attributes: optional JSON value
    }

    VectorSetPage {
      elements: Array<VectorSetElement>,
      cursor: optional string,
      has_more: boolean
    }

    VectorSimilarityQuery {
      by_element: optional string,
      by_vector: optional Array<number>,
      by_vector_base64: optional string,
      count: number,
      with_attributes: boolean
    }

    VectorSimilarityMatch {
      name: string,
      score: number,
      attributes: optional JSON value
    }

VectorSimilarityQuery 必须且只能设置 by_element、by_vector、by_vector_base64 中的一个。数值向量只允许有限的 JSON number；二进制向量在 IPC 中使用 base64；属性保持 JSON 值但必须经过大小限制和可序列化校验。

## 5. 后端架构

### 5.1 模块划分

在现有 src-tauri/src/redis 下新增专用模块：

- array.rs：Array 命令参数、响应解析、摘要、范围/扫描、写操作和聚合。
- vector_set.rs：Vector Set 命令参数、向量编码/解码、分页、属性和相似度查询。
- 必要时在现有 key_ops.rs 中补充摘要读取分派，但不把两种新类型的全部逻辑塞入通用 key 操作文件。

在 src-tauri/src/redis/connection_manager.rs 的 RedisOperations trait 中增加对应的类型安全方法；实现继续复用当前活跃连接和连接锁，不新建旁路连接，也不把原始 Redis 客户端对象暴露给 Tauri 层。

在 src-tauri/src/commands 中增加 Array 和 Vector Set 命令，并在 commands/mod.rs、lib.rs 注册。命令只接收 domain 输入并返回 domain 输出或结构化错误。

### 5.2 最小命令面

后端 API 至少覆盖：

Array：

- get_array_summary
- get_array_range
- scan_array
- set_array_element
- append_array_elements
- delete_array_elements
- search_array
- aggregate_array
- create_array

Vector Set：

- get_vector_set_summary
- list_vector_set_elements
- get_vector_set_element
- create_vector_set
- add_vector_set_elements
- set_vector_set_attributes
- delete_vector_set_attributes
- delete_vector_set_elements
- search_vector_set
- download_vector_embedding

命令名称可以在实现阶段按当前仓库命名风格微调，但不能退化为一个接收任意 Redis 命令的通用入口。每个命令需要有对应的参数、返回 DTO 和错误测试。

### 5.3 Redis 命令与协议

Array 使用目标 Redis Array 命令集合中的必要子集：ARSET、ARMSET、ARGET、ARMGET、ARLEN、ARCOUNT、ARGETRANGE、ARSCAN、ARNEXT、AROP、ARGREP、ARDEL、ARDELRANGE、ARINSERT、ARRING 和 ARINFO。实现不需要为了 UI 暴露未使用命令，但必须为命令名称和参数顺序提供单元测试。

Vector Set 使用 VADD、VCARD、VINFO、VRANGE、VRANDMEMBER、VEMB、VGETATTR、VSETATTR、VREM 和 VSIM。涉及 WITHATTRIBS 等版本差异时，后端根据能力或命令返回值选择兼容形式，并把最终结果归一化为同一个 DTO。

解析器必须同时覆盖 RESP2 和 RESP3 的常见返回结构：

- bulk string、simple string、integer、double；
- Array 和 nested Array；
- RESP3 Map、Set、Attribute；
- 空值和空集合；
- 向量二进制 bulk string。

解析器只提取 DTO 所需字段。未知的可选字段可以忽略；缺少必需字段、类型不匹配或无法解析为规范数字时，返回结构化 CommandFailed，不把原始协议树传给前端。

### 5.4 64 位索引与二进制向量

- Array 索引在 Rust 中使用 u64 或等价无符号 64 位类型，在 IPC/TypeScript 中使用规范十进制字符串。
- 禁止用 JavaScript number 保存 Array 索引，也禁止隐式 parseInt 后再回传服务端。
- 解析索引时拒绝负数、前导符号、溢出和非十进制字符；规范化后再传给 UI。
- Vector Set FP32 向量在 Rust 中校验字节长度是 4 的倍数、维度与 key 配置一致、每个值为有限数；传给前端时使用 base64。
- 数字向量与 base64 向量在同一请求中互斥；服务端不能因为前端绕过校验而接受含 NaN/Infinity 的输入。

## 6. 安全上限与资源保护

所有上限在 Rust domain 层定义并由前端同步使用；前端限制只是体验优化，不是安全边界。

| 项目 | 上限 |
| --- | ---: |
| 单次 Array 范围/扫描返回元素数 | 500 |
| Array 搜索返回元素数 | 500 |
| Array 聚合输入元素数 | 500 |
| Array 单次批量写入元素数 | 500 |
| Array 文本元素长度 | 1 MiB |
| Array 索引字符串长度 | 20 字节 |
| 单次 Vector Set 列表返回元素数 | 200 |
| Vector Set 相似度 top-k | 200 |
| Vector Set 单元素属性大小 | 64 KiB |
| Vector Set 单次批量写入元素数 | 200 |
| 单个向量维度 | 4096 |
| 单个 FP32 向量传输大小 | 4 MiB |
| 单个 IPC 响应 JSON 大小 | 4 MiB |

超出上限返回 InvalidInput，不得自动扩大限制、截断写入或静默丢弃元素。范围分页必须返回 has_more 或等价游标信息，避免 UI 误以为已经加载完整数据。

任何遍历多个 key 的已有或新增逻辑都必须先批量拿到需要的 key/类型信息，再执行批量操作；禁止在循环中逐条查询 SQL。该批次不新增 SQL 功能。

## 7. 错误模型

新增 API 复用现有结构化错误风格，并至少区分：

- UnsupportedFeature：连接能力快照不支持 Array/Vector Set；
- UnsupportedDataType：key 的实际类型与请求 API 不匹配；
- InvalidInput：索引、范围、向量、top-k、属性或批量大小不合法；
- KeyNotFound：目标 key 不存在；
- CommandFailed：Redis 命令返回错误、协议解析失败或服务端响应缺少必需字段；
- ConnectionClosed/现有连接错误：活跃连接已失效。

空列表、空范围和搜索无匹配是合法结果，不应包装成错误。模块不支持和 key 类型不匹配必须可被前端识别，以便展示不同的引导文案。

## 8. 前端架构与状态

### 8.1 Tauri 调用层

在 src/lib/types.ts 增加上述 DTO、能力字段和 RedisValue 新变体；在 src/lib/tauri.ts 增加对应的 typed wrapper。wrapper 必须保持参数对象稳定，不让组件直接拼接命令参数。

### 8.2 Browser 集成

- browserState 将 array 和 vector-set 映射为新的 key 类型；
- KeyDetails 根据实际类型选择 Array 或 Vector Set 专用详情组件；
- AddKey 增加 Array 和 Vector Set 创建入口；
- key 类型过滤器显示新类型；
- 刷新、切换连接、切换 key 时取消或忽略旧请求，不能将旧连接/旧 key 的结果写入当前详情页；
- 加载、空结果、能力不可用、类型冲突、服务端错误均有独立 UI 状态；
- 切换分页、范围或查询条件后清理不再适用的选中项和结果，避免对旧数据执行删除。

本批次不增加新的顶层 Workspace。Array 和 Vector Set 都是 Browser key-detail 能力，沿用现有导航和连接上下文。

### 8.3 组件边界

建议新增：

- src/features/browser/ArrayDetails.tsx
- src/features/browser/VectorSetDetails.tsx
- src/features/browser/arrayState.ts
- src/features/browser/vectorSetState.ts

组件只负责表单、表格和交互状态；协议兼容、数值校验、响应归一化和业务上限由 Tauri/Rust 层负责。若仓库现有组件组织方式更适合拆分为 array/、vector-set/ 子目录，可在实现阶段保持相同边界调整路径。

## 9. 测试策略

实现必须遵循先写失败测试、再写最小实现、最后重构的 TDD 顺序。

### 9.1 Rust 单元测试

- Array 索引解析、规范化、范围校验和各安全上限；
- Vector Set 数字向量、FP32/base64、维度和有限数校验；
- Array 与 Vector Set 命令参数构造；
- RESP2/RESP3 的摘要、列表、属性、相似度和空结果解析；
- 可选字段缺失、必需字段缺失、未知字段、错误回复；
- ModuleCapabilities 对命令集合的判断；
- RedisValue 新变体与已有 key 类型分派。

### 9.2 前端测试

- capabilities false 时不可执行操作并展示不支持状态；
- Array 稀疏索引在范围/扫描结果中保持原始索引；
- Array 编辑、追加、删除和聚合成功/失败状态；
- Vector Set 三种查询来源互斥；
- Vector Set 维度、top-k 和属性大小校验；
- 切换连接或 key 后，旧请求结果不会覆盖当前状态；
- RedisValue/DTO 的序列化与反序列化。

### 9.3 Redis 集成测试

使用现有项目的可选 Redis 集成测试约定，优先复用 REDIX_TEST_REDIS_STACK_URL；未配置时测试应被明确标记为 skipped/ignored，而不是失败。集成测试至少覆盖：

- Array 稀疏索引、范围、扫描、写入、删除、搜索和聚合；
- Vector Set 创建、添加、列表、属性、删除和三种 VSIM 查询；
- RESP2/RESP3（若测试服务可切换）；
- 不支持模块的普通 Redis 连接仍可正常使用。

## 10. 文档与验收

实现完成后同步更新：

- README 中的非 Cloud 功能列表；
- docs/non-cloud-scope.md 的已实现/待实现矩阵；
- 必要的开发者文档和测试环境说明。

验收标准：

1. 在支持对应模块的 Redis 上，Browser 可以完成本设计列出的核心读写、删除和查询流程。
2. 在不支持对应模块的 Redis 上，现有功能可用，新增功能显示结构化“不支持”，无连接崩溃。
3. RESP2/RESP3 解析测试通过，Array 64 位索引未发生精度损失。
4. 向量不会以不可控的原始二进制或任意 Redis 响应泄漏到前端；大小、维度和数值均经过服务端校验。
5. 前端旧请求不会污染新连接或新 key 的状态。
6. Rust、前端、构建、非 Cloud 范围检查和相关集成测试均按仓库现有命令通过。
7. 不引入 Redis Cloud 功能，也不改变现有 Search、JSON、Workbench、Observability 等模块的行为。

## 11. 后续拆分建议

本 spec 审阅通过后，另行生成实现计划并按以下顺序执行：

1. 先落地 domain 类型、错误和解析器测试；
2. 接入能力探测和 RedisValue 摘要；
3. 实现 Array 后端与 Browser 详情；
4. 实现 Vector Set 后端与 Browser 详情；
5. 完成集成测试、文档和全量验证；
6. 再单独为 RedisJSON 深层树、拓扑连接和 Workbench 高级能力撰写设计。
