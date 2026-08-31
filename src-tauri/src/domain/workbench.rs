use crate::{error::AppError, persistence::VersionedJsonDocument};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct CommandResult {
    pub kind: String,
    pub value: serde_json::Value,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct CommandArgument {
    pub name: String,
    pub required: bool,
    pub hint: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct CommandDefinition {
    pub name: String,
    pub summary: String,
    pub arguments: Vec<CommandArgument>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct ExecuteCommandsInput {
    pub connection_id: String,
    pub commands: Vec<String>,
    pub continue_on_error: bool,
}

impl ExecuteCommandsInput {
    pub fn validate(&self) -> Result<(), AppError> {
        if self.connection_id.trim().is_empty()
            || self.commands.is_empty()
            || self.commands.len() > 100
            || self
                .commands
                .iter()
                .any(|command| command.trim().is_empty())
        {
            return Err(AppError::InvalidConnection);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct CommandExecutionItem {
    pub command: String,
    pub result: Option<CommandResult>,
    pub error_code: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct SaveCommandHistoryInput {
    pub connection_id: String,
    pub entries: Vec<CommandHistoryEntry>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct DeleteCommandHistoryInput {
    pub connection_id: String,
    pub command: String,
    pub created_at: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct ClearCommandHistoryInput {
    pub connection_id: String,
}

impl SaveCommandHistoryInput {
    pub fn validate(&self) -> Result<(), AppError> {
        if self.connection_id.trim().is_empty()
            || self.entries.len() > 100
            || self.entries.iter().any(|entry| {
                entry.connection_id != self.connection_id || entry.command.trim().is_empty()
            })
        {
            return Err(AppError::InvalidConnection);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct CommandHistoryEntry {
    pub connection_id: String,
    pub command: String,
    pub result: Option<CommandResult>,
    pub error_code: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct CommandHistoryDocument {
    pub version: u32,
    pub entries: Vec<CommandHistoryEntry>,
}

impl CommandHistoryDocument {
    pub fn delete_entry(&mut self, input: &DeleteCommandHistoryInput) -> Result<(), AppError> {
        if input.connection_id.trim().is_empty()
            || input.command.trim().is_empty()
            || input.created_at.trim().is_empty()
        {
            return Err(AppError::InvalidConnection);
        }
        self.entries.retain(|entry| {
            !is_sensitive_command(&entry.command)
                && !(entry.connection_id == input.connection_id
                    && entry.command == input.command
                    && entry.created_at == input.created_at)
        });
        Ok(())
    }

    pub fn clear_connection(&mut self, input: &ClearCommandHistoryInput) -> Result<(), AppError> {
        if input.connection_id.trim().is_empty() {
            return Err(AppError::InvalidConnection);
        }
        self.entries.retain(|entry| {
            entry.connection_id != input.connection_id && !is_sensitive_command(&entry.command)
        });
        Ok(())
    }
}

impl Default for CommandHistoryDocument {
    fn default() -> Self {
        Self {
            version: 1,
            entries: Vec::new(),
        }
    }
}

impl VersionedJsonDocument for CommandHistoryDocument {
    fn version(&self) -> u32 {
        self.version
    }

    fn migrate(value: serde_json::Value) -> Result<Self, AppError> {
        let document: Self =
            serde_json::from_value(value).map_err(|_| AppError::PersistenceFailed)?;
        if document.version == 1 {
            Ok(document)
        } else {
            Err(AppError::PersistenceFailed)
        }
    }
}

pub fn command_catalog() -> Vec<CommandDefinition> {
    let mut catalog: Vec<CommandDefinition> = [
        ("PING", "检查 Redis 连接", vec![]),
        ("GET", "读取字符串键", vec![("key", true, "键名")]),
        (
            "SET",
            "写入字符串键",
            vec![("key", true, "键名"), ("value", true, "字符串值")],
        ),
        ("DEL", "删除一个或多个键", vec![("key", true, "键名")]),
        ("EXISTS", "检查键是否存在", vec![("key", true, "键名")]),
        ("TYPE", "查看键类型", vec![("key", true, "键名")]),
        ("TTL", "查看键的秒级 TTL", vec![("key", true, "键名")]),
        ("PTTL", "查看键的毫秒级 TTL", vec![("key", true, "键名")]),
        ("DBSIZE", "查看数据库键数量", vec![]),
        (
            "INFO",
            "查看 Redis 服务信息",
            vec![("section", false, "信息分区")],
        ),
        (
            "SCAN",
            "增量扫描键名",
            vec![("cursor", true, "游标"), ("pattern", false, "匹配模式")],
        ),
    ]
    .into_iter()
    .map(|(name, summary, arguments)| CommandDefinition {
        name: name.into(),
        summary: summary.into(),
        arguments: arguments
            .into_iter()
            .map(|(name, required, hint)| CommandArgument {
                name: name.into(),
                required,
                hint: hint.into(),
            })
            .collect(),
    })
    .collect();
    catalog.extend(module_command_catalog());
    catalog.sort_by(|left, right| left.name.cmp(&right.name));
    catalog
}

pub fn is_sensitive_command(command: &str) -> bool {
    let Ok(arguments) = crate::redis::tokenize_command(command) else {
        return true;
    };
    matches!(
        arguments
            .first()
            .map(String::as_str)
            .unwrap_or_default()
            .to_ascii_uppercase()
            .as_str(),
        "AUTH" | "HELLO" | "ACL" | "CONFIG"
    )
}

// Curated offline syntax, following the local RedisInsight command groups.
// Each tuple is (name, summary, required syntax, optional syntax). Semicolons
// separate arguments; option groups remain intact to avoid implying an order.
fn module_command_catalog() -> Vec<CommandDefinition> {
    [
        ("JSON.GET", "读取 JSON 文档或路径", "key", "INDENT indent;NEWLINE newline;SPACE space;path [path ...]"),
        ("JSON.SET", "设置 JSON 路径值", "key;path;json", "NX | XX"),
        ("JSON.DEL", "删除 JSON 路径", "key", "path"),
        ("JSON.TYPE", "查看 JSON 路径类型", "key", "path"),
        ("JSON.MGET", "读取多个 JSON 文档", "key [key ...];path", ""),
        ("JSON.NUMINCRBY", "增加 JSON 数值", "key;path;number", ""),
        ("JSON.ARRAPPEND", "向 JSON 数组追加值", "key;path;json [json ...]", ""),
        ("JSON.ARRLEN", "查看 JSON 数组长度", "key", "path"),
        ("JSON.OBJKEYS", "读取 JSON 对象字段", "key", "path"),
        ("JSON.CLEAR", "清空 JSON 容器或数值", "key", "path"),
        ("JSON.TOGGLE", "切换 JSON 布尔值", "key;path", ""),
        ("FT.SEARCH", "搜索索引中的文档", "index;query", "NOCONTENT;RETURN count field [field ...];SORTBY field [ASC | DESC];LIMIT offset count;PARAMS nargs name value [name value ...];DIALECT version"),
        ("FT.AGGREGATE", "搜索并聚合结果", "index;query", "LOAD count field [field ...];GROUPBY count property [property ...];REDUCE function nargs arg [arg ...] [AS name];SORTBY nargs property [ASC | DESC];LIMIT offset count;PARAMS nargs name value [name value ...];DIALECT version"),
        ("FT.CREATE", "创建搜索索引", "index;SCHEMA field type [field type ...]", "ON HASH | JSON（在 SCHEMA 前）;PREFIX count prefix [prefix ...]（在 SCHEMA 前）"),
        ("FT.INFO", "查看索引信息", "index", ""),
        ("FT.DROPINDEX", "删除搜索索引", "index", "DD"),
        ("FT.EXPLAIN", "解释搜索查询", "index;query", "DIALECT version"),
        ("FT.PROFILE", "分析搜索执行计划", "index;SEARCH | AGGREGATE;QUERY query", "LIMITED（在 QUERY 前）"),
        ("FT._LIST", "列出搜索索引", "", ""),
        ("FT.ALIASADD", "添加索引别名", "alias;index", ""),
        ("FT.ALIASDEL", "删除索引别名", "alias", ""),
        ("TS.CREATE", "创建时间序列", "key", "RETENTION milliseconds;DUPLICATE_POLICY policy;LABELS label value [label value ...]"),
        ("TS.ADD", "添加时间序列样本", "key;timestamp | *;value", "RETENTION milliseconds;ON_DUPLICATE policy;LABELS label value [label value ...]"),
        ("TS.MADD", "添加多个时间序列样本", "key timestamp value [key timestamp value ...]", ""),
        ("TS.GET", "读取最新时间序列样本", "key", "LATEST"),
        ("TS.RANGE", "按时间范围读取样本", "key;fromTimestamp;toTimestamp", "COUNT count;AGGREGATION aggregator bucketDuration;LATEST"),
        ("TS.REVRANGE", "倒序读取时间样本", "key;fromTimestamp;toTimestamp", "COUNT count;AGGREGATION aggregator bucketDuration"),
        ("TS.MRANGE", "按标签查询多个时间序列", "fromTimestamp;toTimestamp;FILTER expression [expression ...]", "WITHLABELS（在 FILTER 前）;AGGREGATION aggregator bucketDuration（在 FILTER 前）;GROUPBY label REDUCE reducer"),
        ("TS.INFO", "查看时间序列信息", "key", "DEBUG"),
        ("TS.DEL", "删除范围内样本", "key;fromTimestamp;toTimestamp", ""),
        ("TS.QUERYINDEX", "按标签查找时间序列键", "expression [expression ...]", ""),
        ("BF.RESERVE", "创建布隆过滤器", "key;error_rate;capacity", "EXPANSION expansion;NONSCALING"),
        ("BF.ADD", "向布隆过滤器添加元素", "key;item", ""),
        ("BF.MADD", "批量添加布隆过滤器元素", "key;item [item ...]", ""),
        ("BF.EXISTS", "检查布隆过滤器元素", "key;item", ""),
        ("BF.MEXISTS", "检查多个布隆过滤器元素", "key;item [item ...]", ""),
        ("BF.INFO", "查看布隆过滤器信息", "key", ""),
        ("CF.RESERVE", "创建布谷鸟过滤器", "key;capacity", "BUCKETSIZE bucketsize;MAXITERATIONS maxiterations;EXPANSION expansion"),
        ("CF.ADD", "添加布谷鸟过滤器元素", "key;item", ""),
        ("CF.ADDNX", "添加不重复的布谷鸟过滤器元素", "key;item", ""),
        ("CF.EXISTS", "检查布谷鸟过滤器元素", "key;item", ""),
        ("CF.DEL", "删除布谷鸟过滤器元素", "key;item", ""),
        ("CF.COUNT", "统计布谷鸟过滤器元素", "key;item", ""),
        ("CF.INFO", "查看布谷鸟过滤器信息", "key", ""),
        ("CMS.INITBYDIM", "按维度创建 Count-Min Sketch", "key;width;depth", ""),
        ("CMS.INITBYPROB", "按误差创建 Count-Min Sketch", "key;error;probability", ""),
        ("CMS.INCRBY", "增加元素计数", "key;item increment [item increment ...]", ""),
        ("CMS.QUERY", "查询元素估计计数", "key;item [item ...]", ""),
        ("CMS.INFO", "查看 Count-Min Sketch 信息", "key", ""),
        ("TOPK.RESERVE", "创建 Top-K", "key;topk", "width depth decay"),
        ("TOPK.ADD", "添加 Top-K 元素", "key;item [item ...]", ""),
        ("TOPK.INCRBY", "增加 Top-K 元素计数", "key;item increment [item increment ...]", ""),
        ("TOPK.QUERY", "查询 Top-K 元素", "key;item [item ...]", ""),
        ("TOPK.LIST", "列出 Top-K 元素", "key", "WITHCOUNT"),
        ("TOPK.INFO", "查看 Top-K 信息", "key", ""),
        ("TDIGEST.CREATE", "创建 t-digest", "key", "COMPRESSION compression"),
        ("TDIGEST.ADD", "添加 t-digest 样本", "key;value [value ...]", ""),
        ("TDIGEST.QUANTILE", "查询分位数", "key;quantile [quantile ...]", ""),
        ("TDIGEST.CDF", "查询累计分布", "key;value [value ...]", ""),
        ("TDIGEST.MIN", "查询最小样本", "key", ""),
        ("TDIGEST.MAX", "查询最大样本", "key", ""),
        ("TDIGEST.INFO", "查看 t-digest 信息", "key", ""),
        ("VADD", "向向量集合添加元素", "key;VALUES dimension value [value ...];element", "SETATTR json"),
        ("VSIM", "向量相似度查询", "key;ELE element | VALUES dimension value [value ...]", "COUNT count;WITHSCORES;WITHATTRIBS;FILTER expression"),
        ("VRANGE", "读取向量集合元素范围", "key;start;end;count", ""),
        ("VEMB", "读取元素向量", "key;element", "RAW"),
        ("VGETATTR", "读取向量元素属性", "key;element", ""),
        ("VSETATTR", "设置向量元素属性", "key;element;json", ""),
        ("VREM", "删除向量元素", "key;element", ""),
        ("VCARD", "读取向量集合大小", "key", ""),
        ("VDIM", "读取向量维度", "key", ""),
        ("VINFO", "读取向量集合信息", "key", ""),
        ("ARSET", "设置连续数组元素", "key;index;value [value ...]", ""),
        ("ARGET", "读取数组元素", "key;index", ""),
        ("ARMSET", "设置稀疏数组元素", "key;index value [index value ...]", ""),
        ("ARMGET", "读取多个数组元素", "key;index [index ...]", ""),
        ("ARGETRANGE", "读取数组范围", "key;start;end", ""),
        ("ARLEN", "读取数组长度", "key", ""),
        ("ARDEL", "删除数组元素", "key;index [index ...]", ""),
        ("XADD", "追加 Stream 消息", "key;* | id;field value [field value ...]", "MAXLEN [~] count（在 ID 前）"),
        ("XLEN", "查看 Stream 长度", "key", ""),
        ("XRANGE", "读取 Stream 范围", "key;start;end", "COUNT count"),
        ("XREVRANGE", "倒序读取 Stream 范围", "key;end;start", "COUNT count"),
        ("XREAD", "读取一个或多个 Stream", "STREAMS key [key ...] id [id ...]", "COUNT count（在 STREAMS 前）;BLOCK milliseconds（在 STREAMS 前）"),
        ("XREADGROUP", "以消费组读取 Stream", "GROUP group consumer;STREAMS key [key ...] id [id ...]", "COUNT count（在 STREAMS 前）;NOACK（在 STREAMS 前）"),
        ("XACK", "确认待处理 Stream 消息", "key;group;id [id ...]", ""),
        ("XPENDING", "查看待处理 Stream 消息", "key;group", "start end count [consumer]"),
        ("XCLAIM", "转移指定待处理消息", "key;group;consumer;min-idle-time;id [id ...]", "IDLE milliseconds;RETRYCOUNT count;FORCE;JUSTID"),
        ("XAUTOCLAIM", "自动转移待处理消息", "key;group;consumer;min-idle-time;start", "COUNT count;JUSTID"),
        ("XGROUP", "管理 Stream 消费组", "CREATE key group id | DESTROY key group | CREATECONSUMER key group consumer | DELCONSUMER key group consumer", "MKSTREAM（仅 CREATE）"),
        ("XINFO", "查看 Stream 或消费组信息", "STREAM key | GROUPS key | CONSUMERS key group", "FULL [COUNT count]（仅 STREAM）"),
        ("XDEL", "删除 Stream 消息", "key;id [id ...]", ""),
        ("XTRIM", "裁剪 Stream", "key;MAXLEN | MINID;threshold", "~（在 threshold 前）;LIMIT count"),
    ].into_iter().map(|(name, summary, required, optional)| CommandDefinition {
        name: name.into(), summary: summary.into(),
        arguments: required.split(';').filter(|arg| !arg.is_empty()).map(|arg| (arg, true))
            .chain(optional.split(';').filter(|arg| !arg.is_empty()).map(|arg| (arg, false)))
            .map(|(name, required)| CommandArgument { name: name.into(), required, hint: match name {
                "key" => "Redis 键名", "path" => "JSONPath，例如 $ 或 $.profile", "json" => "JSON 字面值；包含空格时请使用引号", "index" => "索引名称或数组位置（取决于命令）", _ => name,
            }.into() }).collect(),
    }).collect()
}

pub fn filter_history_entry(entry: &CommandHistoryEntry) -> Option<CommandHistoryEntry> {
    (!is_sensitive_command(&entry.command)).then(|| entry.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workbench_catalog_provides_module_arguments() {
        let catalog = command_catalog();
        for name in [
            "JSON.GET",
            "FT.SEARCH",
            "TS.RANGE",
            "BF.ADD",
            "CF.ADD",
            "CMS.INCRBY",
            "TOPK.ADD",
            "TDIGEST.QUANTILE",
            "VADD",
            "ARSET",
            "XCLAIM",
        ] {
            let command = catalog.iter().find(|command| command.name == name);
            assert!(command.is_some(), "missing {name}");
            assert!(
                !command.unwrap().arguments.is_empty(),
                "missing arguments for {name}"
            );
        }
    }

    #[test]
    fn workbench_filters_quoted_and_escaped_sensitive_commands() {
        assert!(is_sensitive_command("\"AUTH\" password"));
        assert!(is_sensitive_command("A\\UTH password"));
        assert!(is_sensitive_command("'ACL' SETUSER user >password"));
        assert!(!is_sensitive_command("GET public"));
    }

    #[test]
    fn workbench_history_deletion_is_scoped_and_validated() {
        let entry = |connection: &str, command: &str, date: &str| CommandHistoryEntry {
            connection_id: connection.into(),
            command: command.into(),
            created_at: date.into(),
            result: None,
            error_code: None,
        };
        let mut document = CommandHistoryDocument {
            version: 1,
            entries: vec![
                entry("a", "PING", "today"),
                entry("a", "PING", "yesterday"),
                entry("b", "PING", "today"),
                entry("b", "AUTH secret", "today"),
            ],
        };
        document
            .delete_entry(&DeleteCommandHistoryInput {
                connection_id: "a".into(),
                command: "PING".into(),
                created_at: "today".into(),
            })
            .unwrap();
        assert_eq!(document.entries.len(), 2);
        assert_eq!(document.entries[0].created_at, "yesterday");
        document
            .clear_connection(&ClearCommandHistoryInput {
                connection_id: "a".into(),
            })
            .unwrap();
        assert_eq!(document.entries.len(), 1);
        assert_eq!(document.entries[0].connection_id, "b");
        assert!(document
            .clear_connection(&ClearCommandHistoryInput {
                connection_id: " ".into()
            })
            .is_err());
        assert_eq!(document.entries.len(), 1);
    }
}
