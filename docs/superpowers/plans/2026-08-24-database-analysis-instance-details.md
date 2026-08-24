# Database Analysis 与 Instance 详情 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (- [ ]) syntax for tracking.

**Goal:** 在本地 Redis Standalone 中增加按需 Database Analysis 报告和更完整的 Instance 只读详情，同时保持现有 Browser、Workbench、运维观察和 Stream Consumer Group 能力稳定。

**Architecture:** React 页面通过现有 typed Tauri wrapper 调用 Rust command；Rust command 负责输入校验和固定错误映射，RedisService 在当前已打开的 Standalone 连接上读取 INFO/MODULE LIST，或用 SCAN + 固定批次 pipeline 聚合数据库分析结果。分析报告只保存在当前页面，不引入 SQL、后台长任务或新的持久化仓储。

**Tech Stack:** Rust 2021、Tauri 2、redis 1.5 async client、Tokio、Serde、React 19、TypeScript、Vite、Vitest、Testing Library、现有 CSS token。

**Spec:** docs/superpowers/specs/2026-08-24-database-analysis-instance-details-design.md

## Global Constraints

- 本批只支持当前 Redix 的本地 Redis Standalone TCP 连接，不改变现有 profile、密码钥匙串和连接生命周期。
- 用户必须主动点击“开始分析”；打开连接或进入页面不自动扫描全库。
- 分析只能使用 Redis SCAN 游标和固定大小 pipeline，不使用 KEYS、阻塞式全量枚举或前端逐键 IPC。
- max_keys 由 Rust 校验为 1000..=1_000_000，前端默认 100000；仅当达到上限且 SCAN 游标仍未回到 0 时标记 truncated=true。
- TTL -1 归入 No Expiry，非负 TTL 按固定分组统计，TTL -2 代表键已消失且不进入过期分组。
- 单键 metadata 不可用时保留键计数并将对应字段置空；连接断开或批次协议解析失败才终止整份报告。
- Instance/Analysis 只读，不修改键值、TTL、模块、配置或数据库状态，不保存键值和分析历史。
- 不引入 Redis Cloud、Azure Managed Redis、RDI、AI/Copilot、Telemetry、远程插件、Cluster/Sentinel fan-out、TLS/SSH 或模块专用编辑器。
- 不引入 SQL；更不能在循环遍历中查询 SQL。
- 所有错误继续使用固定 AppError code/message，不把 Redis URI、密码、原始 INFO、原始 RESP 或底层错误文本返回前端。
- 默认在当前 main 分支修改，不创建分支；每个提交信息使用简体中文。

---

## 文件地图

| 文件 | 责任 |
|---|---|
| src-tauri/src/domain/database.rs | 现有实例概览、数据库概览、INFO section 解析；增加 Instance 详情 DTO 和 INFO 派生字段解析。 |
| src-tauri/src/domain/database_analysis.rs | 分析输入、报告 DTO、TTL 分组、类型/命名空间/Top Keys 聚合器。 |
| src-tauri/src/redis/database_analysis.rs | SCAN 游标、固定批次 pipeline、Redis Value 转换和 Instance 详情读取。 |
| src-tauri/src/redis/connection_manager.rs | 扩展 RedisOperations 和 RedisService，绑定当前 connection id。 |
| src-tauri/src/commands/database.rs | 新增 Tauri command，保持 command 层无 Redis 客户端细节。 |
| src-tauri/src/lib.rs / src-tauri/src/commands/mod.rs / src-tauri/src/domain/mod.rs / src-tauri/src/redis/mod.rs | 模块导出和 generate_handler 注册。 |
| src-tauri/tests/database_analysis.rs | 纯领域/parser/aggregator 和 service 边界测试；真实 Redis 流程在 redis_integration.rs 中保持 ignored。 |
| src/lib/types.ts / src/lib/tauri.ts | Rust DTO 的 snake_case TypeScript 镜像和两个 typed wrapper。 |
| src/features/database-analysis/databaseAnalysisState.ts | 分析输入默认值、页面状态、格式化和固定错误文案。 |
| src/features/database-analysis/DatabaseAnalysisPage.tsx | 分析表单、加载/成功/截断/失败状态和结果表格。 |
| src/features/database/DatabasePage.tsx / databaseState.ts | 以 InstanceDetails 替换基础实例请求，增加实例详情面板。 |
| src/App.tsx / src/app.smoke.test.tsx / src/styles.css | “数据库分析”导航、无连接禁用、页面接入和 RedisInsight 风格布局。 |
| README.md / docs/non-cloud-scope.md | 记录新能力和仍然排除的能力。 |

---

### Task 1: 领域 DTO、INFO 派生字段与分析聚合器

**Files:**
- Create: src-tauri/src/domain/database_analysis.rs
- Create: src-tauri/tests/database_analysis.rs
- Modify: src-tauri/src/domain/database.rs
- Modify: src-tauri/src/domain/mod.rs

**Interfaces:**
- Produces AnalyzeDatabaseInput::validate() -> Result<(), AppError>。
- Produces AnalysisKeyMetadata { key, key_type, length, memory_bytes, ttl_seconds }。
- Produces AnalysisAccumulator::new(database, pattern, delimiter, max_keys) -> AnalysisAccumulator、process(&mut self, metadata) 和 finish(self, scanned, processed, truncated) -> DatabaseAnalysisReport。
- Produces InstanceDetails::from_info_and_modules(sections, modules) -> Result<InstanceDetails, AppError>、parse_command_stats(sections) -> Vec<CommandStat>。
- Later tasks consume InstanceDetails、AnalyzeDatabaseInput、DatabaseAnalysisReport 和 AnalysisAccumulator 的字段，不重新定义同名 DTO。

- [ ] **Step 1: 写失败的领域测试**

在 src-tauri/tests/database_analysis.rs 写入以下行为测试；此时导出的类型和函数尚不存在：

~~~rust
use std::collections::HashMap;

use redix_lib::domain::{
    parse_command_stats, AnalysisAccumulator, AnalysisKeyMetadata, AnalyzeDatabaseInput,
    InstanceDetails, ModuleSummary,
};

#[test]
fn validates_analysis_input_limits() {
    let mut input = AnalyzeDatabaseInput {
        connection_id: "local".into(),
        pattern: "*".into(),
        delimiter: ":".into(),
        max_keys: 100_000,
    };
    assert_eq!(input.validate(), Ok(()));

    input.max_keys = 999;
    assert_eq!(input.validate().unwrap_err().code(), "INVALID_INPUT");
    input.max_keys = 100_000;
    input.delimiter = String::new();
    assert_eq!(input.validate().unwrap_err().code(), "INVALID_INPUT");
}

#[test]
fn aggregates_types_namespaces_top_keys_memory_coverage_and_ttl_groups() {
    let mut accumulator = AnalysisAccumulator::new(0, "*".into(), ":".into(), 1000);
    accumulator.process(AnalysisKeyMetadata {
        key: "user:1".into(),
        key_type: "string".into(),
        length: Some(12),
        memory_bytes: Some(128),
        ttl_seconds: -1,
    });
    accumulator.process(AnalysisKeyMetadata {
        key: "user:2".into(),
        key_type: "hash".into(),
        length: Some(4),
        memory_bytes: Some(256),
        ttl_seconds: 120,
    });
    accumulator.process(AnalysisKeyMetadata {
        key: "no-namespace".into(),
        key_type: "stream".into(),
        length: None,
        memory_bytes: None,
        ttl_seconds: -2,
    });

    let report = accumulator.finish(3, 3, false);
    assert_eq!(report.database, 0);
    assert_eq!(report.total_keys.total, 3);
    assert_eq!(report.total_keys.observed, 3);
    assert_eq!(report.total_memory.total, 384);
    assert_eq!(report.total_memory.observed, 2);
    assert_eq!(report.top_namespaces_by_keys[0].namespace, "user");
    assert_eq!(report.top_keys_by_memory[0].key, "user:2");
    assert!(report.expiration_groups.iter().any(|group| group.label == "No Expiry"));
    assert!(!report.progress.truncated);
}

#[test]
fn parses_commandstats_and_optional_instance_metrics_without_raw_text() {
    let mut sections = HashMap::new();
    sections.insert(
        "Server".into(), HashMap::from([("redis_version".into(), "7.2.5".into())]),
    );
    sections.insert(
        "Clients".into(),
        HashMap::from([
            ("connected_clients".into(), "3".into()),
            ("blocked_clients".into(), "1".into()),
        ]),
    );
    sections.insert(
        "Commandstats".into(),
        HashMap::from([(
            "cmdstat_get".into(),
            "calls=4,usec=20,usec_per_call=5.0,rejected_calls=0,failed_calls=1".into(),
        )]),
    );

    let details = InstanceDetails::from_info_and_modules(&sections, vec![ModuleSummary {
        name: "ReJSON".into(),
        version: Some("2.8.10".into()),
    }])
    .unwrap();
    assert_eq!(details.overview.server_version.as_deref(), Some("7.2.5"));
    assert_eq!(details.clients.blocked_clients, Some(1));
    assert_eq!(parse_command_stats(&sections)[0].command, "GET");
}
~~~

- [ ] **Step 2: 运行 RED 测试确认缺失接口**

运行：

~~~bash
cargo test --manifest-path src-tauri/Cargo.toml --test database_analysis
~~~

预期：FAIL，原因是 database_analysis 模块、分析 DTO、聚合器和 InstanceDetails 尚未定义；不要为了让测试通过在测试中添加临时实现。

- [ ] **Step 3: 实现输入校验、DTO 和聚合器**

在 src-tauri/src/domain/database_analysis.rs 实现以下固定约束和结构：

~~~rust
pub const ANALYSIS_MIN_KEYS: u64 = 1_000;
pub const ANALYSIS_MAX_KEYS: u64 = 1_000_000;

impl AnalyzeDatabaseInput {
    pub fn validate(&self) -> Result<(), AppError> {
        if self.connection_id.trim().is_empty()
            || self.pattern.chars().count() > 512
            || self.delimiter.chars().count() == 0
            || self.delimiter.chars().count() > 8
            || !(ANALYSIS_MIN_KEYS..=ANALYSIS_MAX_KEYS).contains(&self.max_keys)
        {
            return Err(AppError::InvalidInput);
        }
        Ok(())
    }
}
~~~

在同一模块先定义以下可序列化 DTO，后续 service、command 和 bridge 直接复用，不在不同层复制结构：

~~~rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct AnalyzeDatabaseInput {
    pub connection_id: String,
    pub pattern: String,
    pub delimiter: String,
    pub max_keys: u64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct AnalysisProgress {
    pub scanned: u64,
    pub processed: u64,
    pub max_keys: u64,
    pub truncated: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct TypeSummary {
    pub r#type: String,
    pub total: u64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct AnalysisSummary {
    pub total: u64,
    pub observed: u64,
    pub types: Vec<TypeSummary>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct AnalysisKeyMetadata {
    pub key: String,
    pub key_type: String,
    pub length: Option<u64>,
    pub memory_bytes: Option<u64>,
    pub ttl_seconds: i64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct AnalysisKey {
    pub key: String,
    pub key_type: String,
    pub length: Option<u64>,
    pub memory_bytes: Option<u64>,
    pub ttl_seconds: Option<i64>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct NamespaceSummary {
    pub namespace: String,
    pub keys: u64,
    pub memory_bytes: u64,
    pub types: Vec<TypeSummary>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct ExpirationGroup {
    pub label: String,
    pub keys: u64,
    pub memory_bytes: u64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct DatabaseAnalysisReport {
    pub database: u8,
    pub pattern: String,
    pub delimiter: String,
    pub progress: AnalysisProgress,
    pub total_keys: AnalysisSummary,
    pub total_memory: AnalysisSummary,
    pub top_keys_by_length: Vec<AnalysisKey>,
    pub top_keys_by_memory: Vec<AnalysisKey>,
    pub top_namespaces_by_keys: Vec<NamespaceSummary>,
    pub top_namespaces_by_memory: Vec<NamespaceSummary>,
    pub expiration_groups: Vec<ExpirationGroup>,
}
~~~

AnalysisAccumulator::process 必须：

1. 增加 total_keys.total 和对应 type count；
2. 只有 memory_bytes: Some 时增加 total_memory.total、total_memory.observed 和对应 type bytes；
3. 只有 key 中存在 delimiter 且 delimiter 前缀非空时更新 namespace；
4. 用固定 15 项窗口维护 Top Keys by length/memory；
5. 按 TTL -1、非负秒数、-2 分别处理 No Expiry、固定时间组和消失键；
6. finish 按 keys/memory 降序输出最多 15 个 namespace 和 Top Keys，progress 使用传入的 scanned/processed/max_keys/truncated。

total_keys.observed 等于成功处理的键数，total_memory.observed 等于存在 memory bytes 的键数；total_memory.total 和其 type totals 的单位是字节。

- [ ] **Step 4: 扩展 InstanceDetails 和 commandstats parser**

在 src-tauri/src/domain/database.rs 增加以下结构，字段名称固定为后续 TypeScript 的 snake_case 镜像：

~~~rust
pub struct InstanceDetails {
    pub overview: InstanceOverview,
    pub clients: ClientDetails,
    pub memory: MemoryDetails,
    pub stats: StatsDetails,
    pub persistence: PersistenceDetails,
    pub replication: ReplicationDetails,
    pub command_stats: Vec<CommandStat>,
}

pub struct CommandStat {
    pub command: String,
    pub calls: Option<u64>,
    pub usec: Option<u64>,
    pub usec_per_call: Option<f64>,
    pub rejected_calls: Option<u64>,
    pub failed_calls: Option<u64>,
}
~~~

按以下固定字段实现其余分组，后续 Rust/TypeScript 名称不可漂移：

~~~rust
pub struct ClientDetails {
    pub connected_clients: Option<u64>,
    pub blocked_clients: Option<u64>,
    pub tracking_clients: Option<u64>,
    pub max_clients: Option<u64>,
}

pub struct MemoryDetails {
    pub used_memory_bytes: Option<u64>,
    pub used_memory_peak_bytes: Option<u64>,
    pub used_memory_rss_bytes: Option<u64>,
    pub mem_fragmentation_ratio: Option<f64>,
    pub allocator_active_bytes: Option<u64>,
    pub allocator_resident_bytes: Option<u64>,
}

pub struct StatsDetails {
    pub instantaneous_ops_per_sec: Option<u64>,
    pub expired_keys: Option<u64>,
    pub evicted_keys: Option<u64>,
    pub hit_rate: Option<f64>,
}

pub struct PersistenceDetails {
    pub loading: Option<bool>,
    pub rdb_last_save_time: Option<u64>,
    pub rdb_changes_since_last_save: Option<u64>,
    pub aof_enabled: Option<bool>,
    pub aof_rewrite_in_progress: Option<bool>,
}

pub struct ReplicationDetails {
    pub role: Option<String>,
    pub connected_replicas: Option<u64>,
    pub master_link_status: Option<String>,
    pub master_repl_offset: Option<u64>,
}
~~~

parse_command_stats 只解析 cmdstat_<name> 键，命令名去掉前缀并转为 ASCII 大写；无效单条 commandstat 跳过，不能把整份 INFO 判为失败。不存在的 INFO 指标使用 None。

- [ ] **Step 5: 运行 GREEN 测试和格式检查**

运行：

~~~bash
cargo test --manifest-path src-tauri/Cargo.toml --test database_analysis
cargo test --manifest-path src-tauri/Cargo.toml --test domain parses_info_sections_and_optional_metrics
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
~~~

预期：新增领域测试全部 PASS，既有 domain 测试保持 PASS，fmt 检查通过。

- [ ] **Step 6: 提交领域层**

~~~bash
git add src-tauri/src/domain/database.rs src-tauri/src/domain/database_analysis.rs src-tauri/src/domain/mod.rs src-tauri/tests/database_analysis.rs
git commit -m "增加数据库分析领域模型和实例详情解析"
~~~

### Task 2: Redis INFO 读取与 SCAN Pipeline 分析服务

**Files:**
- Create: src-tauri/src/redis/database_analysis.rs
- Modify: src-tauri/src/redis/connection_manager.rs
- Modify: src-tauri/src/redis/mod.rs
- Test: src-tauri/tests/database_analysis.rs
- Test: src-tauri/tests/redis_integration.rs

**Interfaces:**
- Adds to RedisOperations:
  - async fn get_instance_details(&self, connection_id: &str) -> Result<InstanceDetails, AppError>
  - async fn analyze_database(&self, input: AnalyzeDatabaseInput) -> Result<DatabaseAnalysisReport, AppError>
- Creates redis::database_analysis::load_instance_details(connection: &mut MultiplexedConnection) -> Result<InstanceDetails, AppError>。
- Creates redis::database_analysis::analyze_connection(connection: &mut MultiplexedConnection, database: u8, input: &AnalyzeDatabaseInput) -> Result<DatabaseAnalysisReport, AppError>。
- Later command tasks call only the two RedisService trait methods; they do not call module helpers directly.

- [ ] **Step 1: 写 Redis service 的失败边界测试**

在 src-tauri/tests/database_analysis.rs 增加：

~~~rust
#[tokio::test]
async fn analysis_and_details_require_an_open_connection() {
    let service = RedisService::new(
        Arc::new(TestProfiles::default()),
        Arc::new(TestSecrets::default()),
    );
    assert_eq!(
        service.get_instance_details("missing").await.unwrap_err(),
        AppError::ConnectionFailed
    );
    assert_eq!(
        service.analyze_database(AnalyzeDatabaseInput {
            connection_id: "missing".into(),
            pattern: "*".into(),
            delimiter: ":".into(),
            max_keys: 1000,
        }).await.unwrap_err(),
        AppError::ConnectionFailed
    );
}
~~~

将已有 TestProfiles/TestSecrets fixture 提取到测试文件的私有模块；fixture 的 profile 列表为空时不建立 Redis socket。此时先在 trait 上引用两个不存在的方法，测试应以编译失败结束。

- [ ] **Step 2: 运行 RED 测试确认 service 接口缺失**

运行：

~~~bash
cargo test --manifest-path src-tauri/Cargo.toml --test database_analysis analysis_and_details_require_an_open_connection
~~~

预期：FAIL，原因是 RedisOperations 尚无两个新方法；记录具体编译错误后进入实现，不重复同一失败命令。

- [ ] **Step 3: 实现 Instance 详情读取**

在 src-tauri/src/redis/database_analysis.rs 实现：

~~~rust
pub(crate) async fn load_instance_details(
    connection: &mut redis::aio::MultiplexedConnection,
) -> Result<InstanceDetails, AppError> {
    let info = redis::cmd("INFO")
        .query_async::<String>(connection)
        .await
        .map_err(map_command_error)?;
    let sections = parse_info_sections(&info);
    let modules = redis::cmd("MODULE")
        .arg("LIST")
        .query_async::<redis::Value>(connection)
        .await
        .map(parse_module_list)
        .unwrap_or_default();
    InstanceDetails::from_info_and_modules(&sections, modules)
}
~~~

将现有 parse_module_list/parse_module_entry 提取为该模块可复用的 pub(crate) parser，保持 RESP2 Array/Set、RESP3 Attribute/Map 兼容。get_instance_details 通过现有 self.connection(connection_id) 获取已打开连接后调用 helper；不删除 get_instance_overview，避免破坏已有 command/bridge。

- [ ] **Step 4: 实现 SCAN + 固定批次 metadata pipeline**

在 analyze_connection 中使用以下顺序：

~~~rust
let mut cursor = 0_u64;
let mut scanned = 0_u64;
let mut processed = 0_u64;
let mut accumulator = AnalysisAccumulator::new(
    database,
    input.pattern.clone(),
    input.delimiter.clone(),
    input.max_keys,
);

loop {
    let (next_cursor, keys): (u64, Vec<String>) = redis::cmd("SCAN")
        .arg(cursor)
        .arg("MATCH")
        .arg(&input.pattern)
        .arg("COUNT")
        .arg(500_i64)
        .query_async(connection)
        .await
        .map_err(map_command_error)?;
    scanned += keys.len() as u64;
    let remaining = input.max_keys.saturating_sub(processed);
    let batch = keys.into_iter().take(remaining as usize).collect::<Vec<_>>();
    let metadata = load_key_metadata(connection, &batch).await?;
    for item in metadata {
        if item.key_type.is_empty() {
            continue;
        }
        accumulator.process(item);
        processed += 1;
    }
    if processed >= input.max_keys && next_cursor != 0 {
        return Ok(accumulator.finish(scanned, processed, true));
    }
    cursor = next_cursor;
    if cursor == 0 {
        return Ok(accumulator.finish(scanned, processed, false));
    }
}
~~~

load_key_metadata 对每批最多 500 个键执行两次 pipeline：第一条 pipeline 为每键 TYPE、MEMORY USAGE、TTL，解析完类型后第二条只为 string/list/hash/set/zset/stream 追加 STRLEN/LLEN/HLEN/SCARD/ZCARD/XLEN。每次 pipeline 的回复按固定命令顺序切片，nil 转成 None；不要在前端逐键调用 command。Redis 删除键造成的 nil metadata 只让该字段为空。

helper 的精确签名为：

~~~rust
async fn load_key_metadata(
    connection: &mut redis::aio::MultiplexedConnection,
    keys: &[String],
) -> Result<Vec<AnalysisKeyMetadata>, AppError>
~~~

- [ ] **Step 5: 接入 RedisService trait 和实现**

在 connection_manager.rs 导入新 DTO 和 helper，并添加：

~~~rust
async fn get_instance_details(
    &self,
    connection_id: &str,
) -> Result<InstanceDetails, AppError> {
    let mut connection = self.connection(connection_id).await?;
    load_instance_details(&mut connection).await
}

async fn analyze_database(
    &self,
    input: AnalyzeDatabaseInput,
) -> Result<DatabaseAnalysisReport, AppError> {
    input.validate()?;
    let profile = self.profile(&input.connection_id)?;
    let mut connection = self.connection(&input.connection_id).await?;
    analyze_connection(&mut connection, profile.database, &input).await
}
~~~

profile() 失败返回现有固定 INVALID_CONNECTION，self.connection() 失败返回现有 CONNECTION_FAILED；不把 profile host/port/password 写入错误。

- [ ] **Step 6: 运行 service 定向测试和格式检查**

运行：

~~~bash
cargo test --manifest-path src-tauri/Cargo.toml --test database_analysis
cargo test --manifest-path src-tauri/Cargo.toml --test commands
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
~~~

预期：未打开连接测试、既有 command tests 和 Rust fmt 均 PASS；真实 Redis 测试暂不在普通命令中自动执行。

- [ ] **Step 7: 提交 Redis service**

~~~bash
git add src-tauri/src/redis/database_analysis.rs src-tauri/src/redis/connection_manager.rs src-tauri/src/redis/mod.rs src-tauri/tests/database_analysis.rs
git commit -m "实现数据库分析扫描和实例详情服务"
~~~

### Task 3: Tauri commands、注册与 TypeScript typed bridge

**Files:**
- Modify: src-tauri/src/commands/database.rs
- Modify: src-tauri/src/commands/mod.rs
- Modify: src-tauri/src/lib.rs
- Modify: src-tauri/tests/commands.rs
- Modify: src/lib/types.ts
- Modify: src/lib/tauri.ts
- Test: src/lib/tauri.test.ts

**Interfaces:**
- Adds command get_instance_details(state, connection_id) -> Result<InstanceDetails, AppError>。
- Adds command analyze_database(state, input: AnalyzeDatabaseInput) -> Result<DatabaseAnalysisReport, AppError>。
- Adds bridge getInstanceDetails(connectionId: string): Promise<InstanceDetails>。
- Adds bridge analyzeDatabase(input: AnalyzeDatabaseInput): Promise<DatabaseAnalysisReport>。
- Workspace gains literal "database-analysis"; later App task consumes this exact literal。

- [ ] **Step 1: 先写 command/bridge 的失败测试**

在 src-tauri/tests/commands.rs 的 command symbol test 中先增加：

~~~rust
let _ = database::get_instance_details;
let _ = database::analyze_database;
~~~

在 src/lib/tauri.test.ts 增加：

~~~ts
it("使用 snake_case 参数调用实例详情和数据库分析 command", async () => {
  const invoke = vi.mocked(core.invoke);
  invoke.mockResolvedValueOnce({ overview: null }).mockResolvedValueOnce({ progress: {} });

  await getInstanceDetails("local");
  await analyzeDatabase({
    connection_id: "local",
    pattern: "user:*",
    delimiter: ":",
    max_keys: 1000,
  });

  expect(invoke).toHaveBeenNthCalledWith(1, "get_instance_details", {
    connection_id: "local",
  });
  expect(invoke).toHaveBeenNthCalledWith(2, "analyze_database", {
    input: {
      connection_id: "local",
      pattern: "user:*",
      delimiter: ":",
      max_keys: 1000,
    },
  });
});
~~~

按当前 bridge 测试的 mock 方式导入 core.invoke 和两个 wrapper；此时导出类型/wrapper 未实现，测试应失败。

- [ ] **Step 2: 运行 RED 测试**

运行：

~~~bash
npm test -- --run src/lib/tauri.test.ts
cargo test --manifest-path src-tauri/Cargo.toml --test commands command_symbols_are_registered
~~~

预期：TypeScript 报告缺少类型/wrapper，Rust 报告 command 不存在。

- [ ] **Step 3: 增加 TypeScript DTO 镜像**

在 src/lib/types.ts 添加与 Rust 完全一致的 snake_case 字段：

~~~ts
export interface AnalyzeDatabaseInput {
  connection_id: string;
  pattern: string;
  delimiter: string;
  max_keys: number;
}

export interface AnalysisProgress {
  scanned: number;
  processed: number;
  max_keys: number;
  truncated: boolean;
}

export interface TypeSummary {
  type: string;
  total: number;
}

export interface AnalysisSummary {
  total: number;
  observed: number;
  types: TypeSummary[];
}

export interface AnalysisKey {
  key: string;
  key_type: string;
  length: number | null;
  memory_bytes: number | null;
  ttl_seconds: number | null;
}

export interface NamespaceSummary {
  namespace: string;
  keys: number;
  memory_bytes: number;
  types: TypeSummary[];
}

export interface ExpirationGroup {
  label: string;
  keys: number;
  memory_bytes: number;
}

export interface DatabaseAnalysisReport {
  database: number;
  pattern: string;
  delimiter: string;
  progress: AnalysisProgress;
  total_keys: AnalysisSummary;
  total_memory: AnalysisSummary;
  top_keys_by_length: AnalysisKey[];
  top_keys_by_memory: AnalysisKey[];
  top_namespaces_by_keys: NamespaceSummary[];
  top_namespaces_by_memory: NamespaceSummary[];
  expiration_groups: ExpirationGroup[];
}
~~~

继续在同一文件添加以下 Instance DTO；字段必须与 Task 1 的 Rust 结构逐项对应，所有 Rust Option 数值/布尔/字符串字段在 TypeScript 中使用 number | null、boolean | null、string | null：

~~~ts
export interface ClientDetails {
  connected_clients: number | null;
  blocked_clients: number | null;
  tracking_clients: number | null;
  max_clients: number | null;
}

export interface MemoryDetails {
  used_memory_bytes: number | null;
  used_memory_peak_bytes: number | null;
  used_memory_rss_bytes: number | null;
  mem_fragmentation_ratio: number | null;
  allocator_active_bytes: number | null;
  allocator_resident_bytes: number | null;
}

export interface StatsDetails {
  instantaneous_ops_per_sec: number | null;
  expired_keys: number | null;
  evicted_keys: number | null;
  hit_rate: number | null;
}

export interface PersistenceDetails {
  loading: boolean | null;
  rdb_last_save_time: number | null;
  rdb_changes_since_last_save: number | null;
  aof_enabled: boolean | null;
  aof_rewrite_in_progress: boolean | null;
}

export interface ReplicationDetails {
  role: string | null;
  connected_replicas: number | null;
  master_link_status: string | null;
  master_repl_offset: number | null;
}

export interface CommandStat {
  command: string;
  calls: number | null;
  usec: number | null;
  usec_per_call: number | null;
  rejected_calls: number | null;
  failed_calls: number | null;
}

export interface InstanceDetails {
  overview: InstanceOverview;
  clients: ClientDetails;
  memory: MemoryDetails;
  stats: StatsDetails;
  persistence: PersistenceDetails;
  replication: ReplicationDetails;
  command_stats: CommandStat[];
}
~~~

- [ ] **Step 4: 实现 commands、注册和 bridge**

在 commands/database.rs 实现：

~~~rust
#[tauri::command(rename_all = "snake_case")]
pub async fn get_instance_details(
    state: tauri::State<'_, AppState>,
    connection_id: String,
) -> Result<InstanceDetails, AppError> {
    state.redis.get_instance_details(&connection_id).await
}

#[tauri::command(rename_all = "snake_case")]
pub async fn analyze_database(
    state: tauri::State<'_, AppState>,
    input: AnalyzeDatabaseInput,
) -> Result<DatabaseAnalysisReport, AppError> {
    state.redis.analyze_database(input).await
}
~~~

将两个 command 同步加入 commands::database re-export 和 tauri::generate_handler!。在 src/lib/tauri.ts 按现有 call<T> 包装：

~~~ts
export function getInstanceDetails(connectionId: string): Promise<InstanceDetails> {
  return call<InstanceDetails>("get_instance_details", {
    connection_id: connectionId,
  });
}

export function analyzeDatabase(
  input: AnalyzeDatabaseInput,
): Promise<DatabaseAnalysisReport> {
  return call<DatabaseAnalysisReport>("analyze_database", { input });
}
~~~

- [ ] **Step 5: 运行 GREEN bridge/command 测试**

运行：

~~~bash
npm test -- --run src/lib/tauri.test.ts
cargo test --manifest-path src-tauri/Cargo.toml --test commands
npm run build
~~~

预期：bridge 参数断言、Rust command 编译/注册和 TypeScript 构建全部 PASS。

- [ ] **Step 6: 提交 IPC 层**

~~~bash
git add src-tauri/src/commands/database.rs src-tauri/src/commands/mod.rs src-tauri/src/lib.rs src-tauri/tests/commands.rs src/lib/types.ts src/lib/tauri.ts src/lib/tauri.test.ts
git commit -m "接入数据库分析和实例详情 typed IPC"
~~~

### Task 4: Database Analysis 前端状态与结果页面

**Files:**
- Create: src/features/database-analysis/databaseAnalysisState.ts
- Create: src/features/database-analysis/DatabaseAnalysisPage.tsx
- Create: src/features/database-analysis/databaseAnalysis.test.tsx
- Modify: src/lib/types.ts（仅在 Task 3 遗漏的前端辅助类型处补齐）
- Test: src/lib/tauri.ts wrappers remain covered by Task 3

**Interfaces:**
- Produces initialDatabaseAnalysisState、DEFAULT_ANALYSIS_INPUT、toUserFacingAnalysisError、formatAnalysisNumber 和 formatCoverage。
- DatabaseAnalysisPage props 固定为 { connectionId: string; activeDatabase: number }。
- Page only calls analyzeDatabase(input) once per submit and stores the returned DatabaseAnalysisReport in memory。

- [ ] **Step 1: 写页面 RED 测试**

在 databaseAnalysis.test.tsx mock ../../lib/tauri 的 analyzeDatabase，先写成功、截断、失败和旧响应保护：

~~~tsx
const report: DatabaseAnalysisReport = {
  database: 0,
  pattern: "*",
  delimiter: ":",
  progress: { scanned: 3, processed: 3, max_keys: 100000, truncated: false },
  total_keys: {
    total: 3,
    observed: 3,
    types: [{ type: "string", total: 2 }, { type: "hash", total: 1 }],
  },
  total_memory: {
    total: 384,
    observed: 2,
    types: [{ type: "string", total: 128 }, { type: "hash", total: 256 }],
  },
  top_keys_by_length: [],
  top_keys_by_memory: [
    { key: "user:2", key_type: "hash", length: 4, memory_bytes: 256, ttl_seconds: 120 },
  ],
  top_namespaces_by_keys: [
    { namespace: "user", keys: 2, memory_bytes: 384, types: [{ type: "string", total: 1 }] },
  ],
  top_namespaces_by_memory: [],
  expiration_groups: [{ label: "No Expiry", keys: 1, memory_bytes: 128 }],
};

it("提交默认参数并展示分析摘要和 Top Key", async () => {
  analyzeDatabaseMock.mockResolvedValue(report);
  render(<DatabaseAnalysisPage connectionId="local" activeDatabase={0} />);

  fireEvent.click(screen.getByRole("button", { name: "开始分析" }));
  await waitFor(() => expect(analyzeDatabaseMock).toHaveBeenCalledWith({
    connection_id: "local",
    pattern: "*",
    delimiter: ":",
    max_keys: 100000,
  }));
  expect(await screen.findByText("总键数")).toBeInTheDocument();
  expect(screen.getByText("user:2")).toBeInTheDocument();
});
~~~

继续增加以下三个回归测试，确保错误、截断和请求序号语义可验证：

~~~tsx
it("显示截断提示和固定连接错误", async () => {
  analyzeDatabaseMock.mockResolvedValue({
    ...report,
    progress: { ...report.progress, truncated: true },
  });
  const { rerender } = render(
    <DatabaseAnalysisPage connectionId="local" activeDatabase={0} />,
  );
  fireEvent.click(screen.getByRole("button", { name: "开始分析" }));
  expect(
    await screen.findByText("结果已达到扫描上限，可能不完整"),
  ).toBeInTheDocument();

  analyzeDatabaseMock.mockRejectedValueOnce({ code: "CONNECTION_FAILED" });
  rerender(<DatabaseAnalysisPage connectionId="local-2" activeDatabase={0} />);
  fireEvent.click(screen.getByRole("button", { name: "开始分析" }));
  expect(await screen.findByRole("alert")).toHaveTextContent(
    "无法连接 Redis，请检查连接状态。",
  );
});

it("非法 max_keys 不调用 IPC", () => {
  render(<DatabaseAnalysisPage connectionId="local" activeDatabase={0} />);
  fireEvent.change(screen.getByLabelText("最大扫描键数"), {
    target: { value: "999" },
  });
  fireEvent.click(screen.getByRole("button", { name: "开始分析" }));
  expect(analyzeDatabaseMock).not.toHaveBeenCalled();
  expect(screen.getByRole("alert")).toHaveTextContent("扫描键数必须在 1000 到 1000000 之间。");
});

it("连接切换后忽略旧分析响应", async () => {
  let resolveFirst!: (value: DatabaseAnalysisReport) => void;
  let resolveSecond!: (value: DatabaseAnalysisReport) => void;
  analyzeDatabaseMock
    .mockReturnValueOnce(new Promise<DatabaseAnalysisReport>((resolve) => {
      resolveFirst = resolve;
    }))
    .mockReturnValueOnce(new Promise<DatabaseAnalysisReport>((resolve) => {
      resolveSecond = resolve;
    }));

  const { rerender } = render(
    <DatabaseAnalysisPage connectionId="old" activeDatabase={0} />,
  );
  fireEvent.click(screen.getByRole("button", { name: "开始分析" }));
  rerender(<DatabaseAnalysisPage connectionId="new" activeDatabase={1} />);
  fireEvent.click(screen.getByRole("button", { name: "开始分析" }));

  resolveSecond({ ...report, database: 1, top_keys_by_memory: [] });
  await waitFor(() => expect(screen.getByText("数据库 1")).toBeInTheDocument());
  resolveFirst({
    ...report,
    top_keys_by_memory: [{
      key: "old:key",
      key_type: "string",
      length: 3,
      memory_bytes: 32,
      ttl_seconds: -1,
    }],
  });
  await waitFor(() => expect(screen.queryByText("old:key")).not.toBeInTheDocument());
});
~~~

- [ ] **Step 2: 运行 RED 前端测试**

运行：

~~~bash
npm test -- --run src/features/database-analysis/databaseAnalysis.test.tsx
~~~

预期：FAIL，原因是 state/page 文件和 DatabaseAnalysisPage 尚不存在。

- [ ] **Step 3: 实现 state/helper**

databaseAnalysisState.ts 使用以下初始值，输入控件只接受合法的本地状态：

~~~ts
export const DEFAULT_ANALYSIS_INPUT: AnalyzeDatabaseInput = {
  connection_id: "",
  pattern: "*",
  delimiter: ":",
  max_keys: 100000,
};

export interface DatabaseAnalysisPageState {
  input: AnalyzeDatabaseInput;
  loading: boolean;
  report: DatabaseAnalysisReport | null;
  error: string | null;
}
~~~

toUserFacingAnalysisError 只映射固定文案：INVALID_INPUT 为“分析参数无效。”，CONNECTION_FAILED 为“无法连接 Redis，请检查连接状态。”，COMMAND_FAILED 为“Redis 返回异常，无法完成分析。”，IPC_ERROR 为“调用数据库分析失败，请稍后重试。”；未知 code 返回“数据库分析失败，请稍后重试。”，不使用 error.message。

- [ ] **Step 4: 实现页面与请求序号保护**

页面实现要求：

1. connectionId 变化时重置 input.connection_id、report、error 和 loading；activeDatabase 只用于状态展示。
2. 提交前用 max_keys 数字范围、pattern 长度和 delimiter 长度做即时校验；校验失败不调用 IPC。
3. 每次提交递增 requestRef，只允许最新请求更新状态；卸载时递增 requestRef。
4. 结果显示总键数/observed、总内存/observed、类型表、namespace 表、Top Keys by memory/length 和 expiration group；progress.truncated 显示固定提示“结果已达到扫描上限，可能不完整”。
5. 页面标题显示当前数据库编号（例如“数据库 1”）；只使用 table、列表和 CSS 条形元素，不引入图表包；所有键名作为 React 文本节点渲染。

- [ ] **Step 5: 运行 GREEN 前端测试与构建**

运行：

~~~bash
npm test -- --run src/features/database-analysis/databaseAnalysis.test.tsx
npm run build
~~~

预期：页面测试 PASS，生产构建 PASS。

- [ ] **Step 6: 提交分析页面**

~~~bash
git add src/features/database-analysis src/lib/types.ts
git commit -m "增加数据库分析前端工作区"
~~~

### Task 5: Instance 详情增强与 DatabasePage 回归

**Files:**
- Modify: src/features/database/databaseState.ts
- Modify: src/features/database/DatabasePage.tsx
- Modify: src/features/database/database.test.tsx
- Modify: src/lib/tauri.ts（若 Task 3 的 wrapper 导入排序需调整）

**Interfaces:**
- DatabasePageState.details: InstanceDetails | null 替代仅有 instance: InstanceOverview | null 的页面数据源。
- DatabasePage 初次加载使用 Promise.allSettled([getInstanceDetails(connectionId), getDatabaseOverview(connectionId)])；不再为同一页面重复请求 getInstanceOverview。
- 现有 getInstanceOverview bridge 保留给兼容调用，但不在新页面流程中使用。

- [ ] **Step 1: 更新现有 Database 测试为新 command 并增加详情断言**

将 database.test.tsx 的 mock 从 getInstanceOverview 改为 getInstanceDetails，测试 fixture 包含：

~~~ts
const details: InstanceDetails = {
  overview: instance,
  clients: {
    connected_clients: 3,
    blocked_clients: 1,
    tracking_clients: 0,
    max_clients: 10000,
  },
  memory: {
    used_memory_bytes: 1024,
    used_memory_peak_bytes: 2048,
    used_memory_rss_bytes: 4096,
    mem_fragmentation_ratio: 1.2,
    allocator_active_bytes: null,
    allocator_resident_bytes: null,
  },
  stats: {
    instantaneous_ops_per_sec: 12,
    expired_keys: 4,
    evicted_keys: 0,
    hit_rate: 0.8,
  },
  persistence: {
    loading: false,
    rdb_last_save_time: 1710000000,
    rdb_changes_since_last_save: 2,
    aof_enabled: false,
    aof_rewrite_in_progress: false,
  },
  replication: {
    role: "master",
    connected_replicas: 0,
    master_link_status: null,
    master_repl_offset: null,
  },
  command_stats: [
    { command: "GET", calls: 4, usec: 20, usec_per_call: 5, rejected_calls: 0, failed_calls: 1 },
  ],
};
~~~

增加断言：详情面板显示 blocked_clients、内存峰值、命令 GET 和模块空状态；null 字段显示“不可用”；连接变化仍忽略旧详情/数据库列表响应。

- [ ] **Step 2: 运行 RED 测试**

运行：

~~~bash
npm test -- --run src/features/database/database.test.tsx
~~~

预期：FAIL，因为页面仍调用旧 wrapper、state 没有 details 字段且详情面板不存在。

- [ ] **Step 3: 实现 DatabasePage 数据源切换和实例详情面板**

在 state 中加入 details，加载结果按以下规则保存：

~~~ts
const [instanceResult, databaseResult] = await Promise.allSettled([
  getInstanceDetails(connectionId),
  getDatabaseOverview(connectionId),
]);

const instance = instanceResult.status === "fulfilled" ? instanceResult.value : null;
const databases =
  databaseResult.status === "fulfilled" ? databaseResult.value : [];
const failed = [instanceResult, databaseResult].find(
  (result) => result.status === "rejected",
);
const details = instance;
setState((current) => ({
  ...current,
  loading: false,
  details,
  databases,
  error: failed
    ? toUserFacingDatabaseError(
        failed.status === "rejected" ? failed.reason : undefined,
        databaseLoadFailedMessage,
      )
    : null,
  requestId,
}));
~~~

原有 8 个 overview 指标改为读取 state.details?.overview；在其下方增加 Client/Memory/Stats/Persistence/Replication 面板和 commandstats 表。格式化规则：null 显示“不可用”，布尔值显示“是/否”，比例显示百分比，时间/字节复用现有 helper。

- [ ] **Step 4: 运行 GREEN Database 测试**

运行：

~~~bash
npm test -- --run src/features/database/database.test.tsx
npm run build
~~~

预期：Database 测试和生产构建 PASS；数据库切换失败仍保留旧数据库并显示固定错误。

- [ ] **Step 5: 提交 Instance 详情页面**

~~~bash
git add src/features/database/DatabasePage.tsx src/features/database/databaseState.ts src/features/database/database.test.tsx
git commit -m "增强实例详情展示和数据库概览"
~~~

### Task 6: 应用导航、样式、范围文档与 Smoke 回归

**Files:**
- Modify: src/App.tsx
- Modify: src/app.smoke.test.tsx
- Modify: src/styles.css
- Modify: README.md
- Modify: docs/non-cloud-scope.md

**Interfaces:**
- AppSection accepts database-analysis through the Workspace union。
- navigationItems contains { id: "database-analysis", label: "数据库分析", description: "键空间分析" }。
- Connected app renders DatabaseAnalysisPage with connectionId=activeProfile.id and activeDatabase=activeProfile.database。
- Unconnected app keeps this item disabled and routes to connection page if accessed programmatically。

- [ ] **Step 1: 写导航 RED 回归测试**

在 src/app.smoke.test.tsx 增加：

~~~tsx
it("未连接时禁用数据库分析，连接后显示可用入口", async () => {
  render(<App />);
  expect(screen.getByRole("button", { name: "数据库分析" })).toBeDisabled();
});
~~~

沿用现有 smoke 测试的 bridge mocks；如果已有测试使用同一个名称，将断言合并，避免重复匹配。

- [ ] **Step 2: 运行 RED 测试**

运行：

~~~bash
npm test -- --run src/app.smoke.test.tsx
~~~

预期：FAIL，因为导航没有“数据库分析”按钮。

- [ ] **Step 3: 接入页面和导航**

在 App.tsx：

1. 导入 DatabaseAnalysisPage；
2. 复用现有 database 图标，不新增 SVG 分支；在 NavigationItem 的 icon union 增加 database-analysis 作为该入口的类型值，保证 aria-label 唯一；
3. 在 navigationItems、sectionDescriptions 中加入中文入口；
4. 在主内容条件分支中渲染 DatabaseAnalysisPage；
5. canAccessLocalResources 不增加该入口，使其依赖活动连接；
6. 连接打开后保留现有默认落点 Browser，不自动跳转分析。

- [ ] **Step 4: 增加 RedisInsight 风格样式**

在 src/styles.css 增加 .database-analysis-page、.analysis-form、.analysis-summary-grid、.analysis-table、.analysis-progress、.instance-detail-grid、.instance-command-table 样式，复用现有颜色 token、边框、焦点环、响应式断点和 reduced-motion。窄屏下表格只在面板内部横向滚动，禁止 body 级横向溢出。

- [ ] **Step 5: 更新产品边界文档**

README 增加：Database Analysis 为显式触发、SCAN + pipeline、默认 100000 上限、只读不落盘；Instance 详情包含 INFO 分组和 commandstats。docs/non-cloud-scope.md 记录该能力仍仅支持 Standalone，并保留 Redis Cloud/Azure/RDI/AI/Telemetry/拓扑/模块编辑器排除项。

- [ ] **Step 6: 运行 GREEN smoke、构建和范围扫描**

运行：

~~~bash
npm test -- --run src/app.smoke.test.tsx
npm run test:frontend
npm run build
npm run check:non-cloud
~~~

预期：导航 smoke、全量前端测试、构建和非 Cloud 扫描均 PASS。

- [ ] **Step 7: 提交应用接入**

~~~bash
git add src/App.tsx src/app.smoke.test.tsx src/styles.css README.md docs/non-cloud-scope.md
git commit -m "接入数据库分析导航并完善实例详情样式"
~~~

### Task 7: 真实 Standalone 集成、全量验证与交付记录

**Files:**
- Modify: src-tauri/tests/redis_integration.rs
- Modify: findings.md
- Modify: progress.md
- Modify: task_plan.md

**Interfaces:**
- Integration test calls RedisOperations::get_instance_details and RedisOperations::analyze_database with an explicit unique pattern and max 1000。
- Delivery record lists exact test commands and whether the ignored Redis flow ran；no test result may be described as real Redis success if the environment did not allow it.

- [ ] **Step 1: 写 ignored 的真实 Redis 分析流程**

在现有 redis_integration.rs 的 unique-key flow 中增加 string/hash/list/stream 键后调用：

~~~rust
let details = service
    .get_instance_details("integration")
    .await
    .map_err(|error| error.code().to_owned())?;
assert!(details.overview.server_version.is_some());

let report = service
    .analyze_database(AnalyzeDatabaseInput {
        connection_id: "integration".into(),
        pattern: format!("{}:*", keys.prefix),
        delimiter: ":".into(),
        max_keys: 1000,
    })
    .await
    .map_err(|error| error.code().to_owned())?;
assert!(!report.total_keys.types.is_empty());
assert!(report.top_namespaces_by_keys.iter().any(|item| item.namespace == "redix"));
assert!(report.progress.processed > 0);
~~~

清理仍使用该测试已有的唯一前缀和 DEL，不调用 KEYS，测试函数保持 #[ignore]。

- [ ] **Step 2: 运行 Rust 普通回归**

~~~bash
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo test --manifest-path src-tauri/Cargo.toml
~~~

预期：所有普通 Rust 测试 PASS；真实 Redis 集成项保持 ignored。

- [ ] **Step 3: 在环境允许时运行 ignored Redis 流程**

~~~bash
REDIX_TEST_REDIS_URL=redis://127.0.0.1:6379 cargo test --manifest-path src-tauri/Cargo.toml --test redis_integration -- --ignored --nocapture
~~~

如果 Redis 不可达、权限不足或沙箱拒绝访问，记录具体错误并保留 ignored，不修改测试为强制依赖外部服务。

- [ ] **Step 4: 运行完整交付矩阵**

~~~bash
npm run test:frontend
npm run build
npm run check:non-cloud
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo test --manifest-path src-tauri/Cargo.toml
git diff --check
~~~

再做生产源码静态核对：

~~~bash
rg -n '\bKEYS\b|redis-cli|REDIS_CLOUD|cloud-api|azure|telemetry|copilot' src src-tauri scripts package.json
~~~

命中文档说明、测试名称或既有范围扫描白名单时逐项判断；生产代码不得新增 Cloud/SQL/KEYS 入口。若构建需要外部依赖，先记录错误再按环境权限处理，不能伪造通过。

- [ ] **Step 5: 更新规划记录并完成最终检查**

在 progress.md 记录每个任务的提交、测试数量、ignored/真实 Redis 结果和错误；在 findings.md 追加最终功能对照；在 task_plan.md 将 Phase 12 的 Database Analysis/Instance 子批次标记完成，并保留后续连接拓扑、TLS/SSH、模块编辑器为未完成范围。

- [ ] **Step 6: 提交交付记录**

~~~bash
git add src-tauri/tests/redis_integration.rs findings.md progress.md task_plan.md
git commit -m "完成数据库分析与实例详情验证"
~~~

## 验收矩阵

| 需求 | 负责任务 | 验收证据 |
|---|---|---|
| Instance INFO 分组详情 | Task 1–3、5 | DTO/parser 测试、Database 页面测试、get_instance_details ignored 集成 |
| SCAN + pipeline 分析 | Task 1–2 | 聚合器测试、service 测试、真实 Redis 分析报告 |
| 类型/内存/命名空间/Top Keys/TTL | Task 1、4 | 聚合器字段断言、页面表格和截断状态测试 |
| max_keys 与 truncated 语义 | Task 1、4 | 上限校验、达到上限/SCAN 结束两种测试 |
| typed IPC 与 snake_case | Task 3 | bridge invoke 参数断言、Tauri command 测试 |
| 连接切换/卸载旧响应保护 | Task 4–5 | React deferred Promise 回归测试 |
| 无 Cloud/SQL/KEYS 新入口 | Task 6–7 | check:non-cloud、静态 rg、README 范围说明 |
| 不影响既有功能 | Task 5–7 | 全量前端、Rust、build、fmt、diff check |
