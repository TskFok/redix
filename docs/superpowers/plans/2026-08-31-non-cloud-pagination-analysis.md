# 非 Cloud 分页、聚合与分析历史实施计划

**目标：** 继续对照本地 RedisInsight，补齐大集合增量操作、Stream 分页、Search 聚合和分析历史；不将其他差异声明完成。
**架构：** 保留 React + typed Tauri IPC + Rust RedisService；新增独立模块，公共注册由主任务统一处理。
**技术：** 现有 Rust/redis、React/TypeScript、Vitest；不新增依赖。
**依据：** 用户本轮补全要求、docs/redisinsight-feature-matrix.md、既有非 Cloud 设计。

## 约束

当前 main 分支直接修改，目标仓库只读，不自动提交推送。排除 Redis Cloud，不引入 SQL，不使用 KEYS。用户只排除 Cloud，其他未覆盖项目继续保留在差异矩阵中。所有 Redis 请求受条数/输入/输出限制，错误固定脱敏；切连接/键/库后的旧响应不得污染页面。不因对比而操作用户现有 Redis 数据，集成测试使用隔离临时实例。

## 交付与验证

- [x] Stream：独立 stream_entries domain/service/command/typed API 与 StreamEntries 页面；XRANGE/XREVRANGE 单页至多 500，明确前后游标与排序，XADD/XDEL 增量动作不重建 Stream。先验证边界、超过 500 条分页、切键竞态，再接入详情。
- [x] 大集合：独立 collection domain/service/command/typed API 与 CollectionDetails；HSCAN/SSCAN/ZSCAN 和 LRANGE 分页，HSET/HDEL、SADD/SREM、ZADD/ZREM、LSET/追加增量操作不 DEL 重建。先验证未加载数据和 TTL 保留、分页/输入边界与切键竞态。
- [x] Search 聚合：独立 search_aggregate 模块和 AggregatePanel；受限字段/分组/聚合/排序参数构建 FT.AGGREGATE，拒绝任意额外命令；兼容 RESP2/RESP3、有界结果表和分页，模块缺失局部降级。验证参数、解析、上限和旧响应。
- [x] 分析历史：本机版本化 JSON 显式保存报告，按连接与数据库隔离，限制记录数和文档大小；支持查看、删除和与当前报告比较。报告可能包含键名，保存前在 UI 提示；不自动分析或保存，不保存凭据或键值。验证持久化、隔离、并发写保护、损坏文件与切库竞态。
- [x] 主任务注册新增模块和 IPC、审查接入，运行前端/Rust 全量、生产构建、隔离 Redis 实机、fmt/non-cloud/diff；更新 README/矩阵/进度并列明未覆盖能力。
