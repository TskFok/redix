# Redix 非 Cloud 范围

本文档是人工审查边界，不属于生产代码扫描输入。生产检查只读取 `src/`、`src-tauri/src/` 和 `package.json`，因此本文件中的排除项说明不会被误判为产品入口。

## 允许项

- 自管理 Redis Standalone、Sentinel 与 Cluster DB 0；Cluster 支持普通命令 slot 路由、跨 primary 的 UTF-8 键名完整 `SCAN`、拓扑摘要和 primary-only Database Analysis。二进制键名使对应节点返回可重试失败并保留游标，不承诺任意键空间都能完成遍历。
- `ssh2` transport 使用严格 known_hosts，支持 Agent、Password、内存 PrivateKey 和本机 identity file；Standalone/Sentinel 可与 TLS 组合并保留原目标 SNI。TCP 建连、握手和远端认证共享 6 秒预算，Agent 最多按顺序尝试 32 个身份；本地 Agent IPC 仍可能不可中断并占用 worker、permit 与 CLI 生命周期锁，不能称认证全程均可取消。Cluster+SSH 禁用。
- 连接配置导入导出；普通导出只包含可迁移 profile 元数据，不包含密码、CA PEM、客户端证书、SSH 私钥/口令或 SSH 本机路径。
- 使用系统钥匙串保存本地连接密码；前端 DTO 和连接列表不暴露密码。
- Standalone TLS 的启用/关闭、服务端证书校验、自定义 CA 和 mTLS；TLS 材料继续保存在本机安全存储，不写普通 JSON 文档。
- Browser 使用 `SCAN`、`MATCH`、`COUNT` 分页列出键，并读取键类型、TTL 和值。
- Browser 支持新增键、重命名、批量删除、元数据刷新、类型过滤、显式刷新和校验后的本地 JSON 导入导出。
- String 的有界原始字节读取（最多4MiB，截断只读），以及 Hash、List、Set、Sorted Set 的有界分页和原位字段/成员/索引编辑；原位操作不以当前页重建键，保留未加载数据和 TTL。Stream 按 ID 范围每页最多读取 500 条，支持有界添加与显式选中删除。
- Stream Consumer Group 支持创建/删除 Group、读取消费者与 Pending 列表、确认 Pending 条目、删除消费者和显式 XCLAIM；Claim 单次最多 500 个具体 ID，指定消费者与最小空闲毫秒，高级转移可显式指定 FORCE/IDLE/TIME/RETRYCOUNT；Pending 支持范围/消费者过滤及分页，每页最多500条。
- RedisJSON 根文档的读取和编辑，以及路径级 `JSON.GET`/`JSON.SET`/`JSON.DEL`/`JSON.ARRAPPEND`；未安装 RedisJSON 模块时返回稳定的 `UNSUPPORTED_DATA_TYPE` 错误。
- Redis Array 第一批本地能力：连续/稀疏创建、`ARGETRANGE`/`ARSCAN` 读取、`ARSET` 编辑与追加、`ARDEL`/`ARDELRANGE` 删除、`ARGREP` 搜索和 `AROP` 聚合；数组索引保持十进制字符串，范围、批量和响应大小受固定上限约束。
- Redis Vector Set 第一批本地能力：`VADD` 创建与批量添加、`VCARD`/`VINFO` 摘要、`VRANGE` 分页、`VEMB`/`VGETATTR` 读取、`VSETATTR` 属性编辑、`VREM` 删除、FP32 向量下载和 `VSIM` 相似度查询；维度、元素数、属性、top-k 和响应大小受固定上限约束。
- 连接级 `MODULE LIST` 与命令集能力探测、session 级缓存；探测失败或未检测到 RedisJSON、Array 或 Vector Set 时，不阻断普通 Browser 流程，只让对应模块操作稳定降级为不可用提示。
- RedisSearch / Query 第一批本地能力：在检测到 Search 2.0+ 时支持 `FT._LIST`、`FT.CREATE`、`FT.INFO`、`FT.DROPINDEX`、Hash/JSON 索引管理、受限的 `FT.SEARCH ... LIMIT` 分页查询（默认 NOCONTENT，可选择返回文档字段）和 typed `FT.AGGREGATE` LOAD/GROUPBY/REDUCE/SORTBY/LIMIT 分页，以及 Browser Hash/JSON 键详情中的索引关联摘要；输入、索引数量、结果数量和响应大小均有固定上限，查询文本不持久化。模块缺失或版本不满足时仅 Search / Query 工作区局部降级。
- Workbench 在已打开的本地连接上执行单条或多条 Redis 命令，并展示结构化结果、Raw/Text/JSON 格式、复制入口和遇错继续策略。
- Workbench 命令目录是 Rust 内置静态 DTO；历史按连接写入版本化 `workbench-history.json`，不使用 `localStorage`，AUTH、HELLO、ACL、CONFIG 命令族不落盘。
- Standalone/Sentinel CLI 使用专用持久 socket，保留 MULTI/EXEC、WATCH 和 SELECT 状态；Cluster CLI 只允许普通路由命令，事务、认证/协议切换、数据库选择、订阅、复制、连接模式和 `SCRIPT DEBUG` 等会话状态命令在网络发送前返回 `UNSUPPORTED_FEATURE`。Workbench single/batch 使用相同的 Cluster 门控。
- Database 工作区读取本地实例与数据库键空间概览，并支持安全的数据库切换；指标不可用时按字段降级，不暴露 Redis 原始错误。
- Instance 详情按 INFO 分组展示客户端、内存、统计、持久化、复制指标及 commandstats；全部为当前连接的只读请求。
- Database Analysis 是显式触发的有界 `SCAN`/pipeline 汇总；Cluster 仅汇总 primary 节点并报告失败节点，Standalone/Sentinel 保持当前数据库语义。报告默认不落盘，用户可在键名敏感提示后显式保存到版本化本机 JSON；历史按连接和数据库隔离，每库最多 20 条、全局最多 50 条。
- Query Library 使用版本化 `query-library.json` 保存普通 Redis 命令，支持新增、编辑、删除、搜索和回填 Workbench；敏感命令不保存。
- 设置使用版本化 `settings.json` 保存主题、结果格式、Browser 扫描数量和批量命令遇错策略，Rust 与前端均执行范围校验。
- Slow Log 在 Standalone/Sentinel 支持读取、清空和配置；Cluster typed 四端点在网络操作前拒绝，避免把驱动的随机/全节点回复误称为明确节点作用域。
- Pub/Sub 与 Profiler 在 Standalone/Sentinel 使用独立可取消 socket/transport；Cluster 的 typed 订阅、发布和 Profiler 明确拒绝。命令工作台原生命令仅遵循驱动路由，不提供跨节点保证。
- React/Tauri 本地 UI、前端测试、Rust 单元测试和本地构建工具链。

上述响应上限只覆盖已解码结构、解析、typed IPC 或展示层；Redis 传输层原始 RESP 字节/分配上限尚未实现，不能把解析后的限制描述为原始读取上限。

## 排除项与尚未覆盖能力

- Redis Cloud、Azure Managed Redis、RDI、AI/Copilot、Telemetry/Analytics 及其他云托管 Redis 产品。
- 云登录、云账户、云 SDK、云 API、云端点和云数据库发现。
- 尚未实现 Cluster+SSH、Cluster typed Slow Log/PubSub/Profiler、Sentinel 无缝故障迁移，以及拓扑 DTO 中每节点 version/mode/totalkeys 等目标额外指标。
- 尚未实现的 Redis 模块专用数据类型、模块查询、模块可视化和模块编辑器（RedisJSON 根文档/路径第一批、RedisSearch / Query、Array、Vector Set 与 Stream 基础能力除外）。
- Profiler 自动历史持久化和完整拓扑 fan-out 尚未覆盖。目标Browser源码未发现专用 `XREADGROUP`/`XAUTOCLAIM` 工作流，不再将其列为必须补齐的对比差异。
- Monaco、远程插件、远程插件运行时、云端命令目录和 SQL。
- 其他未实现的运营分析能力和模块专用编辑器；Slow Log / Pub/Sub / 基础 Profiler 已按本文件允许项实现。

## 人工审查清单

- [x] 生产扫描目录固定为 `src/`、`src-tauri/src/`、`package.json`，不包含文档范围说明。
- [x] 入口词扫描大小写不敏感，命中后打印文件和词并以非零状态退出。
- [x] Browser 代码路径使用 `SCAN` 分页，没有加入 `KEYS` 命令。
- [x] 未加入云 SDK、云端点、云登录、云账户模型或云凭据存储。
- [x] 未引入 SQL，也没有在循环中查询 SQL。
- [x] Search / Query 只通过 typed IPC 使用受限 `FT.*` 命令，不使用 `KEYS`、SQL 或未约束的 raw reply。
- [x] Workbench 历史使用共享版本化 JSON 仓储，不保存密码、URI 或底层错误文本。
- [x] 现有前端和 Rust 测试仍需通过；真实 Redis、Tauri bundle 结果按实际环境记录。

## 测试命令

```bash
npm run check:non-cloud
npm run test:frontend
npm run build
npm run test:rust
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
git diff --check
```

### 隔离本机服务测试

下面的命令创建并清理临时服务，不使用现有 Redis 数据库；需要本机监听权限和相应可执行文件。普通测试默认跳过这些依赖外部服务的用例。

```bash
npm run test:redis:local
npm run test:redis:cluster
cargo test --manifest-path src-tauri/Cargo.toml --test cli --test sentinel -- --ignored --nocapture --test-threads=1
cargo test --manifest-path src-tauri/Cargo.toml --lib redis::ssh::tests -- --ignored --nocapture --test-threads=1
```

`test:redis:local` 覆盖普通 Redis 的流程数见最新交付记录，另有 3 个 Redis Stack 流程在缺少环境变量时只记录 early-skip。`test:redis:cluster` 先运行 launcher 安全单测，再随机保留三组 client/cluster-bus 端口，启动三个仅 loopback 的 owned Redis 子进程；`CLUSTER CREATE` 前后均用 `INFO` 校验 PID，且只清理自身子进程和临时目录，不继承用户的 `REDIX_TEST_REDIS*` 地址。

CLI/Sentinel 需要 `redis-server`。真实 SSH fixture 位于 `src-tauri/tests/ssh.rs`，使用外部隔离 SSH/Redis 服务的 `REDIX_TEST_SSH_HOST`、`REDIX_TEST_SSH_PORT`、`REDIX_TEST_SSH_USERNAME`、`REDIX_TEST_SSH_KNOWN_HOSTS`、Redis host/port 及所选认证字段；本轮未运行真实 sshd fixture。SSH 单元测试不代表已通过 Linux/Windows 原生端到端测试。

`.github/workflows/cross-platform.yml` 配置 macOS、Windows、Ubuntu 的前端、Rust、Web 与 native no-bundle 构建；macOS/Linux 安装 Redis 后执行隔离网络流程，Windows 显式跳过缺少 Redis 可执行文件的流程。CI 配置不等于 CI 已运行，本轮本机结果只代表 macOS。

### Redis Stack 集成补充

- RedisJSON 路径 ignored 集成测试使用可选环境变量 `REDIX_TEST_REDIS_STACK_URL`。
- 若未配置该环境变量，必须把 Redis Stack 流程记录为 skip/未执行，不能写成真实网络通过。
- 若已配置，执行：

```bash
cargo test --manifest-path src-tauri/Cargo.toml --test redis_integration redis_stack_json_path_flow_when_redis_stack_is_available -- --ignored --nocapture
```

RedisSearch / Query ignored 流程同样使用 `REDIX_TEST_REDIS_STACK_URL`：

```bash
cargo test --manifest-path src-tauri/Cargo.toml --test redis_integration redis_stack_search_flow_when_redis_stack_is_available -- --ignored --nocapture
```

Array / Vector Set ignored 流程同样使用 `REDIX_TEST_REDIS_STACK_URL`，会按服务端实际命令能力分别执行并清理唯一测试键：

```bash
cargo test --manifest-path src-tauri/Cargo.toml --test redis_integration redis_stack_array_and_vector_set_flow_when_redis_stack_is_available -- --ignored --nocapture
```

## 2026-08-31 增补

- JSON 树与键前缀树只操作已有读取/扫描结果，保留路径白名单与节点上限。
- Workbench 增加模块目录、当前行补全与注释、树/表结果以及历史删除；敏感命令过滤使用与执行一致的 tokenizer。
- 观察面板支持用户显式导出 Profiler LOG、Pub/Sub JSON、Slow Log CSV/JSON；不自动落盘，导出前显示敏感参数风险提示，CSV 做公式转义。
- 暂停显示不会停止后台订阅/MONITOR，缓存上限保持 5000/10000。
- 本文历史“排除”中尚未实现的非 Cloud 能力，统一在 `docs/redisinsight-feature-matrix.md` 记录为待补差异，不能视作已完成。

## 2026-09-07 增补

允许本地内置值解码、受限Worker协议展示、选中键后台删除、Stream高级操作、VECTOR字段配置与可视过滤、连接标签、版本化查询包，以及只读分析建议/历史趋势；具体资源上限与剩余差异见功能矩阵和本轮交付记录。没有新增云入口、远程插件、SQL或自动修改Redis的推荐动作。


## 2026-09-07 继续补齐

增加typed KNN/二进制PARAMS、字段AS别名、可取消后台分析、当前页面实例趋势、Cluster额外INFO指标、TimeSeries/Geo内置结果图表、PHP结构化只读解码及快捷键操作面板。分析与删除任务只在应用会话保留，不承诺重启恢复。没有增加远程嵌入服务、远程插件或自动执行建议。

`npm run test:redis:stack` 使用本机Docker的官方Redis 8.4.5镜像，创建具有唯一标签的临时容器，只发布loopback随机端口，无主机数据挂载。运行前比对容器与端口的Redis run_id，验证模块与持久化设置；测试后按容器ID和标签清理。本次注册中心拒绝镜像下载，所以模块实测尚未执行；脚本安全单测通过不等于Redis模块端到端通过。
