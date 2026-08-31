# 非 Cloud 功能差异续作实施计划

**目标：** 以当前本地 RedisInsight 源码为参照，在既有 Redix 架构中补齐缺失能力，明确记录尚未覆盖的差异。
**架构：** 沿用 React feature + typed Tauri command + Rust domain/service，按模块独立交付并统一回归。
**依据：** docs/superpowers/specs/2026-08-24-redisinsight-non-cloud-parity-modules-topology-design.md 与本轮用户补全要求。

## 约束

- 当前 main 分支；参考仓库只读，不自动提交/推送。
- 排除 Redis Cloud；凭据留在系统安全存储，错误使用固定 DTO。
- 不引入 SQL，不使用 KEYS 全量扫描，不让模块不可用阻塞基础 Browser。
- 先写关键行为失败测试，完成对应实现后运行定向测试；统一执行 npm test、cargo test、build 和范围检查。

## 独立交付项

1. Workbench：在 src/features/workbench、src-tauri/src/domain/workbench.rs 和 commands/workbench.rs 增强模块目录、当前行帮助、嵌套结果以及历史清理。验证光标定位、复杂 RESP 结果、敏感历史和连接切换。
2. JSON 树：在 src/features/browser 中增加树形视图，选中节点回填已有路径编辑器。特殊字段名必须使用现有 JSONPath 白名单可接受的编码，不能可编辑地指向另一个字段。验证深层对象/数组、特殊键、超大文档与切键状态。
3. Sentinel：连接 profile 添加可选拓扑配置，使用现有 redis crate Sentinel 能力或明确 SENTINEL 主节点发现流程；连接管理必须保留真实拓扑身份和当前 master 的连接信息。测试旧配置迁移、凭据边界、种子失败回退、只连接 master。
4. Stream Claim：已有 Consumer Group 面板补 XCLAIM typed API，指定目标消费者、最小空闲时间和明确选中的 ID，仅显式点击执行。测试限额、命令参数、失败后状态保持与切换竞态。
5. 核实剩余 SSH/Cluster、搜索结果、Profiler 和 Browser 差异，按真实证据更新矩阵；不能将未实现项声明完成。

## 统一验收

执行前端、Rust、生产构建、cargo fmt、non-cloud 和 git diff --check。可以启动隔离的本地 Redis 临时实例时运行真实网络测试；缺少模块/拓扑环境必须明确区分编译通过、模拟通过和真实服务通过。
