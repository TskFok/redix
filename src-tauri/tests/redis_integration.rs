use std::{
    collections::{BTreeMap, HashMap},
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};

use redix_lib::{
    domain::{
        ConnectionProfile, CreateKeyInput, DeleteKeysInput, ExecuteCommandsInput, ExportKeysInput,
        ExportedKey, HashEntry, ImportKeysInput, KeyInfoInput, KeyValue, RedisValue,
        RenameKeyInput, ScanKeysInput, SetKeyInput, SetKeyTtlInput, SortedSetEntry, StreamEntry,
        StreamField,
    },
    error::AppError,
    persistence::{ProfileRepository, SecretStore},
    redis::{RedisOperations, RedisService},
};

struct TestProfiles {
    profiles: Vec<ConnectionProfile>,
}

impl ProfileRepository for TestProfiles {
    fn load(&self) -> Result<Vec<ConnectionProfile>, AppError> {
        Ok(self.profiles.clone())
    }

    fn save(&self, _profiles: &[ConnectionProfile]) -> Result<(), AppError> {
        Ok(())
    }
}

#[derive(Default)]
struct TestSecrets {
    values: Mutex<HashMap<String, String>>,
}

impl SecretStore for TestSecrets {
    fn read(&self, connection_id: &str) -> Result<Option<String>, AppError> {
        Ok(self
            .values
            .lock()
            .expect("test secret lock must not be poisoned")
            .get(connection_id)
            .cloned())
    }

    fn write(&self, connection_id: &str, password: &str) -> Result<(), AppError> {
        self.values
            .lock()
            .expect("test secret lock must not be poisoned")
            .insert(connection_id.to_owned(), password.to_owned());
        Ok(())
    }

    fn delete(&self, connection_id: &str) -> Result<(), AppError> {
        self.values
            .lock()
            .expect("test secret lock must not be poisoned")
            .remove(connection_id);
        Ok(())
    }
}

fn integration_profile(url: &str) -> (ConnectionProfile, Option<String>) {
    let client = redis::Client::open(url).expect("REDIX_TEST_REDIS_URL must be a valid redis URL");
    let info = client.get_connection_info();
    let (host, port) = match info.addr() {
        redis::ConnectionAddr::Tcp(host, port) => (host.clone(), *port),
        _ => panic!("REDIX_TEST_REDIS_URL must use standalone TCP"),
    };
    let password = info.redis_settings().password().map(str::to_owned);
    let profile = ConnectionProfile {
        id: "integration".into(),
        name: "Integration".into(),
        host,
        port,
        username: info.redis_settings().username().map(str::to_owned),
        database: info.redis_settings().db() as u8,
        has_password: password.is_some(),
    };
    (profile, password)
}

struct TestKeys {
    prefix: String,
    string: String,
    hash: String,
    list: String,
    set: String,
    zset: String,
    stream: String,
    json: String,
    rename_source: String,
    rename_target: String,
    batch_a: String,
    batch_b: String,
    import_string: String,
    import_hash: String,
}

impl TestKeys {
    fn unique() -> Self {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock must be after Unix epoch")
            .as_nanos();
        let prefix = format!("redix:task4:{}:{suffix}", std::process::id());

        Self {
            prefix: prefix.clone(),
            string: format!("{prefix}:string"),
            hash: format!("{prefix}:hash"),
            list: format!("{prefix}:list"),
            set: format!("{prefix}:set"),
            zset: format!("{prefix}:zset"),
            stream: format!("{prefix}:stream"),
            json: format!("{prefix}:json"),
            rename_source: format!("{prefix}:rename-source"),
            rename_target: format!("{prefix}:rename-target"),
            batch_a: format!("{prefix}:batch-a"),
            batch_b: format!("{prefix}:batch-b"),
            import_string: format!("{prefix}:import-string"),
            import_hash: format!("{prefix}:import-hash"),
        }
    }

    fn all(&self) -> [&str; 13] {
        [
            &self.string,
            &self.hash,
            &self.list,
            &self.set,
            &self.zset,
            &self.stream,
            &self.json,
            &self.rename_source,
            &self.rename_target,
            &self.batch_a,
            &self.batch_b,
            &self.import_string,
            &self.import_hash,
        ]
    }
}

fn check_key_value(
    actual: &KeyValue,
    key: &str,
    key_type: &str,
    value: &RedisValue,
) -> Result<(), String> {
    if actual.key == key && actual.key_type == key_type && actual.value == *value {
        Ok(())
    } else {
        Err(format!(
            "{key_type} key read-back does not match the value written by the test"
        ))
    }
}

async fn run_redis_flow(service: &RedisService, keys: &TestKeys) -> Result<(), String> {
    let info = service
        .open_connection("integration")
        .await
        .map_err(|error| error.code().to_owned())?;
    if info.server_version.is_empty() {
        return Err("PING succeeded but returned an empty connection version".into());
    }

    let expected = vec![
        (
            keys.string.as_str(),
            "string",
            5_u64,
            RedisValue::String {
                value: "hello".into(),
            },
        ),
        (
            keys.hash.as_str(),
            "hash",
            1_u64,
            RedisValue::Hash {
                fields: vec![HashEntry {
                    field: "field".into(),
                    value: "value".into(),
                }],
            },
        ),
        (
            keys.list.as_str(),
            "list",
            2_u64,
            RedisValue::List {
                items: vec!["a".into(), "b".into()],
            },
        ),
        (
            keys.set.as_str(),
            "set",
            1_u64,
            RedisValue::Set {
                members: vec!["member".into()],
            },
        ),
        (
            keys.zset.as_str(),
            "zset",
            1_u64,
            RedisValue::SortedSet {
                members: vec![SortedSetEntry {
                    member: "member".into(),
                    score: 1.5,
                }],
            },
        ),
    ];

    for (key, key_type, _, value) in &expected {
        let stored = service
            .set_key(SetKeyInput {
                connection_id: "integration".into(),
                key: (*key).to_owned(),
                value: value.clone(),
            })
            .await
            .map_err(|error| error.code().to_owned())?;
        check_key_value(&stored, key, key_type, value)?;

        let fetched = service
            .get_key("integration", key)
            .await
            .map_err(|error| error.code().to_owned())?;
        check_key_value(&fetched, key, key_type, value)?;
    }

    let ttl = service
        .set_key_ttl(SetKeyTtlInput {
            connection_id: "integration".into(),
            key: keys.string.clone(),
            ttl_ms: 5_000,
        })
        .await
        .map_err(|error| error.code().to_owned())?;
    if !(1..=5_000).contains(&ttl) {
        return Err("string TTL is outside the requested range".into());
    }
    let ttl_value = service
        .get_key("integration", &keys.string)
        .await
        .map_err(|error| error.code().to_owned())?;
    if !(1..=5_000).contains(&ttl_value.ttl_ms) {
        return Err("get_key did not return the configured string TTL".into());
    }

    let mut cursor = 0;
    let mut summaries = BTreeMap::new();
    loop {
        let page = service
            .scan_keys(ScanKeysInput {
                connection_id: "integration".into(),
                cursor,
                pattern: format!("{}:*", keys.prefix),
                count: 1,
                key_type: None,
            })
            .await
            .map_err(|error| error.code().to_owned())?;
        summaries.extend(
            page.keys
                .into_iter()
                .map(|summary| (summary.key.clone(), summary)),
        );
        cursor = page.cursor;
        if cursor == 0 {
            break;
        }
    }
    for (key, key_type, size, _) in &expected {
        let summary = summaries
            .get(*key)
            .ok_or_else(|| format!("SCAN did not return {key_type} test key"))?;
        if summary.key_type != *key_type || summary.size != Some(*size) {
            return Err(format!("SCAN metadata does not match {key_type} test key"));
        }
    }

    let filtered = service
        .scan_keys(ScanKeysInput {
            connection_id: "integration".into(),
            cursor: 0,
            pattern: format!("{}:*", keys.prefix),
            count: 100,
            key_type: Some("hash".into()),
        })
        .await
        .map_err(|error| error.code().to_owned())?;
    if filtered
        .keys
        .iter()
        .any(|summary| summary.key_type != "hash")
    {
        return Err("SCAN type filtering returned a non-hash key".into());
    }

    let exported = service
        .export_keys(ExportKeysInput {
            connection_id: "integration".into(),
            keys: vec![keys.string.clone(), keys.hash.clone()],
        })
        .await
        .map_err(|error| error.code().to_owned())?;
    if exported.len() != 2 {
        return Err("Browser export did not return both source keys".into());
    }
    let imported_entries = exported
        .into_iter()
        .map(|entry| ExportedKey {
            key: if entry.key == keys.string {
                keys.import_string.clone()
            } else {
                keys.import_hash.clone()
            },
            ..entry
        })
        .collect::<Vec<_>>();
    let imported = service
        .import_keys(ImportKeysInput {
            connection_id: "integration".into(),
            entries: imported_entries.clone(),
        })
        .await
        .map_err(|error| error.code().to_owned())?;
    if imported != 2 {
        return Err("Browser import did not write both new keys".into());
    }
    let repeated = service
        .import_keys(ImportKeysInput {
            connection_id: "integration".into(),
            entries: imported_entries,
        })
        .await
        .map_err(|error| error.code().to_owned())?;
    if repeated != 0 {
        return Err("Browser import overwrote an existing key".into());
    }
    if service
        .get_key("integration", &keys.string)
        .await
        .map_err(|error| error.code().to_owned())?
        .key
        != keys.string
    {
        return Err("Browser import changed an original source key".into());
    }

    let command = service
        .execute_command("integration", "PING")
        .await
        .map_err(|error| error.code().to_owned())?;
    if command.kind != "string" || command.value != serde_json::json!("PONG") {
        return Err("Workbench PING did not return the expected result".into());
    }

    let stopped_batch = service
        .execute_commands(ExecuteCommandsInput {
            connection_id: "integration".into(),
            commands: vec![
                "PING".into(),
                "DBSIZE".into(),
                "NO_SUCH_COMMAND".into(),
                "PING".into(),
            ],
            continue_on_error: false,
        })
        .await
        .map_err(|error| error.code().to_owned())?;
    if stopped_batch.len() != 3
        || stopped_batch[0]
            .result
            .as_ref()
            .map(|item| item.value.clone())
            != Some(serde_json::json!("PONG"))
        || stopped_batch[1]
            .result
            .as_ref()
            .map(|item| item.kind.as_str())
            != Some("number")
        || stopped_batch[2].error_code.as_deref() != Some("COMMAND_FAILED")
    {
        return Err("Workbench batch did not stop on the first command error".into());
    }

    let continued_batch = service
        .execute_commands(ExecuteCommandsInput {
            connection_id: "integration".into(),
            commands: vec!["PING".into(), "NO_SUCH_COMMAND".into(), "DBSIZE".into()],
            continue_on_error: true,
        })
        .await
        .map_err(|error| error.code().to_owned())?;
    if continued_batch.len() != 3
        || continued_batch[1].error_code.as_deref() != Some("COMMAND_FAILED")
        || continued_batch[2]
            .result
            .as_ref()
            .map(|item| item.kind.as_str())
            != Some("number")
    {
        return Err("Workbench batch did not continue after a command error".into());
    }

    let stream_value = RedisValue::Stream {
        entries: vec![
            StreamEntry {
                id: "1-0".into(),
                fields: vec![StreamField {
                    field: "event".into(),
                    value: "created".into(),
                }],
            },
            StreamEntry {
                id: "2-0".into(),
                fields: vec![StreamField {
                    field: "event".into(),
                    value: "updated".into(),
                }],
            },
        ],
    };
    let stream = service
        .create_key(CreateKeyInput {
            connection_id: "integration".into(),
            key: keys.stream.clone(),
            value: stream_value.clone(),
            ttl_ms: None,
        })
        .await
        .map_err(|error| error.code().to_owned())?;
    check_key_value(&stream, &keys.stream, "stream", &stream_value)?;

    let stream_fetched = service
        .get_key("integration", &keys.stream)
        .await
        .map_err(|error| error.code().to_owned())?;
    check_key_value(&stream_fetched, &keys.stream, "stream", &stream_value)?;

    let renamed = service
        .create_key(CreateKeyInput {
            connection_id: "integration".into(),
            key: keys.rename_source.clone(),
            value: RedisValue::String {
                value: "rename".into(),
            },
            ttl_ms: None,
        })
        .await
        .map_err(|error| error.code().to_owned())?;
    check_key_value(
        &renamed,
        &keys.rename_source,
        "string",
        &RedisValue::String {
            value: "rename".into(),
        },
    )?;
    let renamed = service
        .rename_key(RenameKeyInput {
            connection_id: "integration".into(),
            key: keys.rename_source.clone(),
            new_key: keys.rename_target.clone(),
        })
        .await
        .map_err(|error| error.code().to_owned())?;
    check_key_value(
        &renamed,
        &keys.rename_target,
        "string",
        &RedisValue::String {
            value: "rename".into(),
        },
    )?;

    for key in [&keys.batch_a, &keys.batch_b] {
        service
            .create_key(CreateKeyInput {
                connection_id: "integration".into(),
                key: key.to_string(),
                value: RedisValue::String {
                    value: "batch".into(),
                },
                ttl_ms: None,
            })
            .await
            .map_err(|error| error.code().to_owned())?;
    }
    let deleted = service
        .delete_keys(DeleteKeysInput {
            connection_id: "integration".into(),
            keys: vec![keys.batch_a.clone(), keys.batch_b.clone()],
        })
        .await
        .map_err(|error| error.code().to_owned())?;
    if deleted != 2 {
        return Err("batch delete did not report two deleted keys".into());
    }
    for key in [&keys.batch_a, &keys.batch_b] {
        if !matches!(service.get_key("integration", key).await, Err(error) if error.code() == "COMMAND_FAILED")
        {
            return Err("batch-deleted key is still readable".into());
        }
    }

    let info = service
        .get_key_info(KeyInfoInput {
            connection_id: "integration".into(),
            key: keys.rename_target.clone(),
        })
        .await
        .map_err(|error| error.code().to_owned())?;
    if info.key != keys.rename_target || info.key_type != "string" || info.size != Some(6) {
        return Err("key info does not include the renamed string metadata".into());
    }

    let json_value = RedisValue::Json {
        value: serde_json::json!({"name": "Alice", "active": true}),
    };
    match service
        .create_key(CreateKeyInput {
            connection_id: "integration".into(),
            key: keys.json.clone(),
            value: json_value.clone(),
            ttl_ms: None,
        })
        .await
    {
        Ok(json) => {
            if !matches!(json.value, RedisValue::Json { .. }) {
                return Err("RedisJSON key did not read back as JSON".into());
            }
            let fetched = service
                .get_key("integration", &keys.json)
                .await
                .map_err(|error| error.code().to_owned())?;
            if fetched.key != keys.json
                || !matches!(
                    fetched.key_type.as_str(),
                    "ReJSON-RL" | "ReJSON-RS" | "JSON"
                )
                || fetched.value != json_value
            {
                return Err("RedisJSON key did not read back the expected document".into());
            }
        }
        Err(error) if error.code() == "UNSUPPORTED_DATA_TYPE" => {}
        Err(error) => return Err(format!("RedisJSON operation failed: {}", error.code())),
    }

    for key in keys.all() {
        service
            .delete_key("integration", key)
            .await
            .map_err(|error| error.code().to_owned())?;
        match service.get_key("integration", key).await {
            Err(error) if error.code() == "COMMAND_FAILED" => {}
            Err(error) => {
                return Err(format!(
                    "deleted key returned {} instead of COMMAND_FAILED",
                    error.code()
                ))
            }
            Ok(_) => return Err("deleted key is still readable".into()),
        }
    }

    Ok(())
}

async fn cleanup_redis_flow(service: &RedisService, keys: &TestKeys) -> Result<(), String> {
    let mut failures = Vec::new();
    for key in keys.all() {
        if let Err(error) = service.delete_key("integration", key).await {
            failures.push(error.code());
        }
    }
    if let Err(error) = service.close_connection("integration").await {
        failures.push(error.code());
    }

    if failures.is_empty() {
        Ok(())
    } else {
        Err(format!("cleanup failed: {}", failures.join(", ")))
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "设置 REDIX_TEST_REDIS_URL 后用 cargo test -- --ignored --nocapture 运行"]
async fn exercises_standalone_redis_operations() {
    let url = std::env::var("REDIX_TEST_REDIS_URL")
        .expect("请设置 REDIX_TEST_REDIS_URL 后运行 Redis 集成测试");
    let (profile, password) = integration_profile(&url);
    let secrets = TestSecrets::default();
    if let Some(password) = password.as_deref() {
        secrets.write("integration", password).unwrap();
    }
    let service = RedisService::new(
        std::sync::Arc::new(TestProfiles {
            profiles: vec![profile],
        }),
        std::sync::Arc::new(secrets),
    );
    let keys = TestKeys::unique();
    let flow = run_redis_flow(&service, &keys).await;
    let cleanup = cleanup_redis_flow(&service, &keys).await;

    match (flow, cleanup) {
        (Ok(()), Ok(())) => {}
        (Err(flow), Ok(())) => panic!("Redis integration flow failed: {flow}"),
        (Ok(()), Err(cleanup)) => panic!("Redis integration cleanup failed: {cleanup}"),
        (Err(flow), Err(cleanup)) => {
            panic!("Redis integration flow failed: {flow}; cleanup also failed: {cleanup}")
        }
    }
}
