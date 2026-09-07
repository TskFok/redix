# 本地功能补齐实施计划

**Goal:** 对照 RedisInsight `48ee19fab` 的真实代码补齐目前存在的本地工作流缺口。
**Architecture:** 保留 Rust + typed Tauri IPC + React；独立领域模块、命令和功能组件，主任务统一注册及验收。
**Spec:** `docs/superpowers/specs/2026-09-01-redisinsight-local-full-parity-design.md`

## 约束

当前 main 直接修改，参考仓库只读，不提交或推送。Redis Cloud 不进入产品。继续现有本地产品边界，不引入 SQL。输出、存储和任务数量固定上限；错误不返回原始凭据或服务器文本。

## 实施与接口

- [x] String 原始字节读取/编辑：`get_string_value`、`set_string_value`、`decode_string_value`、`encode_string_value`，统一 `{input}` 参数。Browser 预览不读取 UTF-8 GET；新组件接管 String 值。字节使用 base64，保存原子验证类型与存在性并保留 TTL；截断数据不能整体覆盖。覆盖空值、二进制、非法编码、压缩输出上限、请求切换和TTL。
- [x] VECTOR 索引：Search 字段新增可选 vector 配置，校验 FLAT/HNSW、TYPE/DIM/DISTANCE_METRIC，构造真实 FT.CREATE 参数；能力检测与执行使用同一快照。新增 Text/Tag/Numeric/Geo 查询构建器，仅显式回填，不自动执行。
- [x] 本地连接标签：独立版本化 `connection-tags.json`，`list_connection_tags`/`save_connection_tags`；key/value 编辑、连接列表过滤，不将标签混入凭据。
- [x] 查询包：`export_query_package`/`import_query_package`，版本化JSON、全量先验后原子追加、500项上限、逐行敏感命令拒绝；与现有查询库写入共享锁，损坏文件不静默覆盖。
- [x] 批量任务：`start_bulk_delete`/`list_bulk_tasks`/`cancel_bulk_task`。每批固定上限、任务总数/并发数有界，绑定原始连接代次，取消或断连后不开始新命令，明确已处理/已删除/失败/未知结果；不在后台自动重试写操作。任务显示跨页面保留。
- [x] Stream：按目标实际源码补齐 XGROUP SETID、Claim 高级参数及 Pending 范围分页。旧矩阵对目标 XAUTOCLAIM/实时消费的推断未经源码证实，不将其当作目标必须具备的工作流。
- [x] Browser 自动刷新和键树可配置分隔符：有界间隔、页面卸载清理、不与编辑写入重叠。

## 验证

每个独立实现先新增关键行为失败测试，再补实现和定向测试。主任务检查各模块差异，统一运行 `npm test`、`cargo test --offline --manifest-path src-tauri/Cargo.toml`、隔离 Redis/Cluster、`npm run build`、non-cloud、Rust fmt 和 diff。所有未执行的模块环境/平台验证明确记录，不据mock宣称实机通过。

## 继续补齐（2026-09-07）

- [x] JSON 草稿生命周期修复：同内容/TTL/模块信息更新保留草稿，切键/真正数据更新重置并废弃旧请求。
- [x] Search 初始化竞态与 typed KNN 查询：服务器 schema 校验维度和浮点类型，二进制 PARAMS，用户显式提供向量；无外部服务。
- [x] 后台数据库分析：原始连接代次、可取消/总超时、跨页面恢复、范围隔离与显式保存报告。
- [x] Workbench 本地内置 TimeSeries/Geo 图表：绑定已执行命令，白名单解析、样本与名称上限、保留原始结果；不引入远程插件。
- [x] 实例概览趋势：当前页面有界内存样本，自动刷新默认关闭，隐藏/操作期间暂停，连接切换隔离。
- [x] Cluster 节点额外指标：从现有 INFO 提取版本/模式/键数等，无额外网络轮询，缺失不当作零。
- [x] 统一前端/Rust/隔离Redis与Cluster/macOS构建验收，更新实际功能矩阵与交付记录。

- [x] PHP结构化只读解码、BOM/二进制键回归，以及快捷键搜索面板与输入法保护。
