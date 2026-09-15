# Redix

Redix 是一个面向自管理 Redis 的桌面客户端，使用 Rust + Tauri + React 构建。当前版本提供连接管理（含导入导出、Standalone、Sentinel、Cluster 与 SSH/TLS 组合）、Browser 键浏览与编辑、RedisJSON 根文档与路径级操作、RedisSearch 索引管理与有限查询、Redis Array/Vector Set 模块数据操作、Stream Consumer Group 观察、数据库/实例概览、Query Library、本地设置、Slow Log、Pub/Sub、基础 Profiler，以及 Workbench 和 CLI。

## 前置条件

- Node.js 20+ 和 npm。
- Rust stable、Cargo，以及 Tauri 所需的系统依赖。
- 一个可访问的本地 Redis Standalone 实例，默认可使用 `redis://127.0.0.1:6379`。
- 若 Redis 开启认证，在连接表单中填写密码；密码只交给本地后端并由系统钥匙串保存，连接列表不回显密码。

## 安装与开发

```bash
npm install
npm run dev
```

`npm run dev` 启动前端开发服务器。需要运行桌面壳时使用：

```bash
npm run tauri dev
```

### WebView 内容安全策略

`src-tauri/tauri.conf.json` 中的 `app.security.csp` 用于打包后的应用：只允许应用自身资源、同源解码 Worker 和 Tauri IPC，禁止内联脚本、动态代码执行（`eval` / `Function`）、iframe、插件对象、表单提交和页面基地址覆盖。React 通过 DOM 样式属性设置的动态布局仍可使用；Redis / SSH / TLS 连接由 Rust 后端建立，无需将服务器地址加入 CSP。

`app.security.devCsp` 仅为开发模式增加 React Refresh 内联脚本、Vite 动态样式及本机 `1421` 端口的 HMR WebSocket 权限。Vite 读取这份配置并发送 CSP 响应头；修改策略后需重启开发服务器，修改 HMR 端口时需同步更新允许的地址。生产策略保留 Tauri 自动添加脚本哈希和样式 nonce 的机制，不启用 `unsafe-eval`。增加资源来源时请按用途收窄对应指令，并验证打包后的应用。

## 测试、检查与构建

```bash
npm run check:non-cloud
npm run test:release
npm run test:frontend
npm run test:rust
npm run test:redis:local # 需要 redis-server 7.4+（覆盖 Hash 字段 TTL）；自动创建并清理隔离实例
npm run test:redis:cluster # 需要 redis-server 与 redis-cli；自动创建并清理隔离三主节点
npm run test:redis:stack # 需要本机 Docker 与预先下载的 redis:8.4.5；隔离模块测试
npm run build
npm run tauri build
```

原有的 `npm run tauri:build` 仍可使用。

## 发布与清理

```bash
npm run release # 默认递增 patch，例如 0.1.0 → 0.1.1
npm run release -- 0.2.0 # 发布指定的更高稳定版本
npm run release -- --current # 强制重打并推送当前版本标签
npm run clean # 清理构建产物与 Vite 缓存，保留已安装依赖
```

发布前必须保持工作区干净，且当前分支与 `origin` 上的同名分支完全同步。`release` 会检查各版本源一致性，运行发布脚本测试、前端测试、Rust 测试与构建检查；通过后同步更新 `package.json`、`package-lock.json`、`src-tauri/tauri.conf.json`、`src-tauri/Cargo.toml` 和 `src-tauri/Cargo.lock`，以中文提交版本变更，推送当前分支与 `vX.Y.Z` 标签。`--current` 保持版本号不变，强制将当前版本的本地和远程标签指向当前提交，适用于重新触发发布。

GitHub Actions 统一使用 `.github/workflows/release.yml`：直接推送 `main` 或其他分支不触发 CI，Pull Request 仅运行跨平台验证。`npm run release` 推送的 `v*` 标签只触发这一个工作流；标签与五个版本文件的一致性预检、macOS/Windows/Linux 跨平台验证全部通过后，才构建 macOS arm64/x64、Windows 和 Linux 安装包，发布至 GitHub Releases 并自动生成发布说明。

`clean` 仅删除 `node_modules/.vite`、`dist` 和 `src-tauri/target`，不会删除 `node_modules` 中已安装的依赖；清理后可直接重新运行开发或构建命令。

## 实现说明

`check:non-cloud` 只扫描产品源码目录和 `package.json`，不扫描 README、设计文档或范围说明。它用于阻止非本地产品入口意外进入代码和菜单文案。

Browser 由后端使用 Redis `SCAN` 遍历当前数据库，跨批次去重后一次性返回所有匹配键，列表在扫描全部成功后显示，无需点击“加载更多”。默认扫描只返回 key，不执行逐键 `TYPE` 或其他元数据查询。选择类型时使用原生 `SCAN TYPE`（Redis 6.0+）重新遍历，返回的类型由筛选条件确定；清空类型重新扫描全部类型，键名过滤和刷新保留当前类型条件。JSON 筛选使用 RedisJSON 原生类型名 `ReJSON-RL`，Vector Set 使用 `vectorset`。类型、TTL、长度、内存等信息和内容在详情中按需读取。平铺和树形均使用虚拟滚动，只挂载可视区域附近的行。每次扫描数量只控制底层 SCAN 的 COUNT 提示，不限制最终键数量。全量结果保存在内存中，等待时间和内存占用随键数量增加；`SCAN TYPE` 仍然遍历键空间，不是类型索引查询。扫描结果不是某一时刻的严格快照，期间新增、删除、过期或类型变化的键可能与打开详情时不同。Cluster 会遍历所有 primary；节点失败或扫描中断时报错，不将部分结果显示为完整列表。键名按原始字节传递，Standalone/Cluster 扫描支持非 UTF-8、NUL、空键及空格键名；二进制键在列表中以 Hex 显示并标记，选择和写操作使用原始字节身份。Workbench 只在当前本地连接上执行用户输入的 Redis 命令。

Browser 支持新增键、重命名、批量删除、元数据刷新、类型过滤、显式刷新以及校验后的本地 JSON 导入导出；基础数据类型支持 String、Hash、List、Set、Sorted Set、Stream。Hash、List、Set、Sorted Set 使用有界分页和字段/成员/索引级增量写入，不用当前页重建整个键；Stream 使用有界 ID 范围分页，并支持显式添加和删除消息。Stream 详情还支持 Consumer Group 的创建/删除、消费者与 Pending 列表、Pending 确认、消费者删除和显式 XCLAIM 转移。转移需要选择消息并指定目标消费者、最小空闲时间，只操作满足条件的 Pending 消息，基础转移不启用 FORCE；高级面板支持 IDLE/TIME/RETRYCOUNT/FORCE，FORCE 必须显式选择并说明会新增 Pending 记录，不自动消费。RedisJSON 同时支持根文档编辑，以及路径级读取、保存、删除和数组追加。连接级模块能力通过 `MODULE LIST` 探测并按 session 缓存；探测失败或未检测到 RedisJSON/RedisSearch 时，不会阻断普通 Browser 流程，对应的路径编辑器或 Search / Query 工作区会稳定降级为不可用提示。RedisSearch / Query 工作区在 Search 2.0+ 可用时支持 `FT._LIST`、`FT.CREATE`、`FT.INFO`、`FT.DROPINDEX`、Hash/JSON 索引和有限的 `FT.SEARCH ... LIMIT` 查询（默认 NOCONTENT，可开启文档字段结果表），以及 typed `FT.AGGREGATE` LOAD/GROUPBY/REDUCE/SORTBY/LIMIT 查询；查询文本不持久化；Browser 的 Hash/JSON 键详情会显示匹配的索引摘要。

Hash 详情支持字段 TTL 的查看、按毫秒设置及移除，需 Redis 7.4+ 和对应命令权限；字段 TTL 批量读取，旧版本仍可浏览和编辑普通 Hash，编辑已有字段时保留其过期时间。整个键的 TTL 仍然生效。List 支持任意正负索引查询（0 为首项，-1 为尾项），查询结果可按绝对索引编辑，并可在确认后按数量删除头部或尾部；删除数量达到或超过长度时，整个键会消失。Sorted Set 默认按分数升序分页，可切换降序或成员匹配扫描；排序由 Redis 对整个集合执行，同分成员按字典顺序排列，降序时一并反转。并发修改可能移动 List 索引或 Sorted Set 的分页位置，操作前可刷新确认。

检测到对应命令集后，Browser 还支持 Redis Array 的连续/稀疏创建、范围读取、扫描、单元格编辑、追加、按索引或区间删除、ARGREP 搜索和 AROP 聚合；Vector Set 支持受限维度的元素创建与批量添加、分页浏览、向量/属性读取与编辑、FP32 向量下载、VSIM 相似度查询和元素删除。两类模块都通过 typed IPC 接入，模块缺失时只禁用对应类型和详情操作，不影响普通 Redis 键浏览；单次批量与响应大小均有固定上限。

Workbench 支持本地内置命令目录、命令前缀提示、多行批量执行、遇错继续策略、Raw/Text/JSON 结果格式和复制。命令历史按连接保存到应用数据目录的版本化 JSON 文件；AUTH、HELLO、ACL、CONFIG 命令族不会写入历史，也不使用 `localStorage`。

Database 工作区提供服务器版本、运行模式、连接数、内存、命令量、命中率和已加载模块等只读概览，并展示数据库键空间统计；实例详情按 INFO 分组展示客户端、内存、统计、持久化与复制指标，并提供 commandstats 命令统计。数据库切换成功后才更新当前连接配置。Database Analysis 是显式触发的只读工具：仅在用户点击“开始分析”后，以 `SCAN` 加固定批次 pipeline 汇总当前数据库；默认最多处理 100000 个键，也不会在连接后自动开始分析。结果默认不落盘；用户可在键名敏感提示后显式保存到版本化本机历史，按连接和数据库隔离，支持查看、删除及相同参数的观察值比较。Query Library 使用应用数据目录中的版本化 `query-library.json` 保存普通 Redis 命令，支持新增、编辑、删除、搜索和回填 Workbench，AUTH、HELLO、ACL、CONFIG 命令族不会保存。设置使用 `settings.json` 持久化主题、结果格式、Browser 扫描数量和批量命令错误策略。

运维观察工作区在 Standalone/Sentinel 上提供 Slow Log 的读取、清空和 `slowlog-*` 配置，独立 Pub/Sub channel/pattern 订阅、发布和实时消息流，以及基于独立 `MONITOR` socket 的 Profiler 实时命令流。Cluster 的 typed Slow Log、Pub/Sub 和 Profiler 均在网络操作前稳定拒绝，因为当前 DTO/会话没有明确节点作用域；命令工作台仍可显式执行原生命令，但路由范围由 Redis 驱动决定，不承诺全拓扑语义。Pub/Sub 每个连接只保留一个可取消会话，前端最多缓存 5000 条消息；Profiler 前端最多缓存 10000 条事件；关闭连接、切换数据库或卸载页面时会清理后台任务。Profiler 启动前会提示 MONITOR 可能带来的性能影响，不自动保存日志文件或历史记录。支持筛选、暂停显示及显式导出：Profiler LOG、Pub/Sub JSON、Slow Log CSV/JSON。暂停只冻结显示，后台仍接收并保留有界缓存；导出包含当前筛选快照，可能带有敏感命令参数。

## 当前边界

本版本支持 Standalone、Sentinel 和 Cluster DB 0。Cluster 已支持普通命令的 slot 路由、跨 primary 的 UTF-8 键名完整 `SCAN`、16384 slot 拓扑摘要及 primary-only Database Analysis；Cluster+SSH、Cluster Pub/Sub、Cluster Profiler 和 typed Cluster Slow Log 明确不支持。拓扑复用节点 INFO 显示版本、运行模式、键总量及内存上限；缺失或损坏数据保持不可用。SSH 由内置 `ssh2` transport 提供 Agent、Password、内存私钥或本机私钥文件认证；Standalone/Sentinel 可与 TLS 组合，Cluster+SSH 禁用。TCP 建连、SSH 握手和远端认证共享 6 秒建连预算，超时或调用方取消会关闭已建立且仍在初始化的自有 socket；Agent 按返回顺序最多尝试 32 个身份且不重置该预算。但锁定的 libssh2 在 Unix/Windows 本地 Agent IPC 中可能同步阻塞，当前进程内接口无法可靠打断该本地等待；卡住时仍可能占用 worker、permit 和 CLI 生命周期锁，不能视为全部 SSH 认证均可超时取消。普通连接导出不包含密码、证书正文、SSH 私钥、口令或 SSH 本机路径。

文中所述响应大小上限约束的是已解码结构、解析、typed IPC 或展示层；当前没有 Redis 传输层原始 RESP 字节/分配上限，服务端超大回复仍可能在解析拒绝前占用内存。

Redis Cloud、Azure Managed Redis、RDI、AI/Copilot、Telemetry/Analytics、远程插件与插件市场永久排除；没有云登录、云账户、云数据库发现、云 SDK 或 SQL。本轮已进一步补齐本地解码、选中键后台删除、Stream 高级操作、VECTOR 索引配置、查询构建、连接标签、查询包和分析建议/历史趋势；剩余差异继续逐项记录在功能矩阵。

除已列出的 RedisJSON、RedisSearch、Array、Vector Set 和 Stream 能力外，其他模块专用数据编辑器、Monaco/插件运行时尚未实现；不能将这些差异视作已对齐。

## 本轮补齐能力（2026-09-01）

- Browser 支持平铺/按 `:` 前缀分层的键树切换；两种视图共用后端全量扫描结果，展开目录不额外查询 Redis。JSON 可按对象/数组折叠、定位并回填路径编辑器，不能安全表达的路径只读。
- Workbench 支持离线模块命令帮助、当前光标行补全、`#`/`//` 注释行、嵌套结果树/表格，以及按连接删除单条或清空历史。敏感命令继续不写历史，包括被引号包围或转义的命令名。
- Search 可显式开启文档字段结果；默认键名模式兼容旧行为。支持 RESP2/RESP3 回复和过期文档空内容；字段、文档、深度、单页和总响应均有限额，超限返回固定错误。
- Sentinel 配置种子节点、master name 和独立认证，支持失败种子回退及主节点 ROLE 校验；重新连接或切库时重新发现。正在运行的会话不会无缝迁移到新的主节点。Sentinel 密码只存系统钥匙串，普通导出不含密码。
- SSH 使用跨平台 `ssh2` transport，支持 Agent、Password、内存 PrivateKey 和本机 identity file；必须用 known_hosts 严格验证主机指纹，未知或变化的主机密钥失败。支持 Standalone/Sentinel 与 TLS 组合，并保留原目标 host 作为 TLS SNI；Cluster+SSH 禁用。SSH 每 15 秒发送保活；隧道失效时，新的 Redis 连接可自动重建隧道，并合并同一客户端的并发恢复请求。恢复仅重试连接初始化，不重放业务命令或恢复已有 CLI 事务、订阅及 MONITOR 会话。已在 macOS 验证自动化测试，以及真实 sshd 下的保活、闲置访问和中断后首次访问恢复；Windows/Linux 尚未验证。
- 大集合和 Stream 详情直接进入分页端点；Hash/List/Set/Sorted Set 原位编辑保留未加载数据与 TTL，键消失或类型变化时拒绝写入；Stream 读写保留 Consumer Group 和 TTL。
- Search 增加受限聚合查询面板，不接受任意聚合管道；Database Analysis 增加显式本机历史和相同扫描参数比较，报告可能包含键名且不自动保存。
- Standalone/Sentinel CLI 通过专用持久 socket 保留多轮 MULTI/EXEC、WATCH 和 SELECT 状态，不影响 Browser 的数据库。Cluster CLI 使用共享路由池，只支持普通无会话状态命令；事务、数据库选择、认证/协议切换、订阅、复制、连接模式和 `SCRIPT DEBUG` 等 socket 状态命令会在发送前稳定拒绝，Workbench 的 single/batch 采用相同门控。关闭、离开页面或主连接切库会清理 CLI；每条命令超时 5 秒后断开且不重试。

完整对比与尚未覆盖的能力见 [功能差异矩阵](docs/redisinsight-feature-matrix.md)。这不是与 RedisInsight 的全量等价实现。

## 本轮新增（2026-09-07）

- String 详情使用原始字节读取，支持 UTF-8、ASCII、Hex、Binary、Base64、JSON 编辑与 Gzip/Zlib/Deflate；MessagePack（含 CSharp LZ4）、PHP serialized 及无 schema Protobuf 为只读展示。单值读取最多4MiB，截断预览禁止整体保存；写入原子检查键存在/类型并保留TTL。普通读取不需要EVAL权限。结构化解码在Worker执行，最多2个并发、2秒期限；解析库的内部预分配不能被解析后节点限制完全覆盖。
- 键树分隔符可配置，列表自动刷新默认关闭，选择键、查看详情或编辑时暂停。选中键可后台删除，支持进度、取消和部分失败；Cluster直接按当前槽位主节点发送，不重试写入。任务仅保留在当前应用会话，最多10000键、4个并发、20条任务记录；取消不能撤回已发送操作，失败项可能已生效。
- Stream增加组位置SETID、按ID范围与消费者分页Pending，以及高级Claim。Search增加完整FLAT/HNSW VECTOR字段配置、版本门控及Text/Tag/Numeric/Geo查询构建器；预览回填后由用户显式执行。
- 连接支持key/value标签与过滤；Query Library支持版本化JSON查询包导入导出，全量校验后原子追加，敏感命令不会导入，损坏存储不被静默覆盖。
- 分析页显示基于保留Top Keys的本地建议，并可显式加载同参数/同节点范围历史趋势；提示采样、截断及失败节点限制，不自动修改Redis配置或数据。后台分析可取消、设定总超时并在切回页面后恢复，绑定原始连接代次；每个连接/数据库一个运行任务，全局最多2个，保留16条会话记录与单份最多1MiB报告，结果须显式保存才落盘。
- Search KNN 从 FT.INFO 校验向量 schema，按 FLOAT32/FLOAT64 编码二进制 PARAMS，限制 K≤200；用户显式输入向量并执行，结果显示键和距离。索引创建可设置 AS 查询别名，使 JSONPath 字段也能用于安全的 KNN 查询；旧查询包和字段输入兼容。
- Workbench 内置 TimeSeries 和 Geo 结果可视化，绑定已执行命令，保留原始结果，最多2000点、20条序列。地理结果为本地坐标散点，无地图底图；不加载远程插件。名称展示上限256字符，数据表分页100行。
- 实例概览可开启2/5/10/30秒自动刷新并查看最近120次内存/操作数/客户端数趋势；默认关闭，后台窗口和切库期间暂停，切连接/离开页面清空样本。
- Ctrl/Cmd+K 打开可搜索的快捷键与操作面板；支持导航、当前输入聚焦、键盘选择及焦点恢复。导航与聚焦快捷键避开输入区和输入法组字，不自动执行 Redis 命令。
- JSON 路径草稿在等价数据、TTL和模块信息刷新时保留；切键、切连接或服务端数据真正变化时正确重置。PHP/Protobuf保留字符串BOM，PHP对象、引用和二进制键按标记数据展示，不创建类实例。

本轮对比、验证和剩余功能见 [交付记录](docs/local-parity-2026-09-07.md)。

## 二进制键与集合（2026-09-14）

Browser 的键名、String 值、Hash 字段名和值、List 值、Set/Sorted Set 成员现在支持任意字节。新建、重命名和集合编辑提供 UTF-8、Hex、Base64 编码选项；格式切换转换现有字节，非法编码会在发送前报错。非 UTF-8、包含控制字节或只有空白的数据默认以 Hex 展示。为防止文本控件改写换行字节，单行输入中的 CR/LF 以及多行输入中的 CR 保持 Hex/Base64 编辑；普通 LF 多行文本仍可使用 UTF-8。空键、空字段、空成员及纯空格数据均可无损表示。Hash 字段 TTL、List 索引查询、集合分页和原位更新保留原有语义与限额。

导入导出的 JSON 兼容现有文本格式：有效 UTF-8 仍为字符串，其余字节表示为 `{"base64":"/wA="}`（示例字节 `ff 00`）。键名及上述值位置都接受这一形式，Base64 使用标准字母表和完整填充；导入按原始字节识别重复键。新建集合时，Hex/Base64 模式使用 JSON 数组，例如 Hash 的 `[{"field":"/w==","value":"AA=="}]`，Set/List 的 `["/w==",""]`，Sorted Set 的 `[{"member":"/w==","score":1}]`。
