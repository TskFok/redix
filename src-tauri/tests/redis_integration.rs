use std::{
    collections::{BTreeMap, HashMap},
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};

use redix_lib::{
    domain::{
        AcknowledgeStreamPendingEntriesInput, AggregateArrayInput, AnalyzeDatabaseInput,
        AppendArrayInput, AppendJsonArrayInput, ArrayAggregateOperation, ArrayCreateMode,
        ArrayPredicate, ArrayRangeInput, ConnectionProfile, CreateArrayInput, CreateKeyInput,
        CreateSearchIndexInput, CreateStreamConsumerGroupInput, CreateVectorSetInput,
        DeleteArrayElementsInput, DeleteArrayRangeInput, DeleteJsonPathInput, DeleteKeysInput,
        DeleteStreamConsumerGroupInput, DeleteStreamConsumerInput, DeleteVectorSetElementsInput,
        ExecuteCommandsInput, ExportKeysInput, ExportedKey, GetJsonPathInput,
        GetKeySearchIndexesInput, GetSlowLogsInput, GetStreamConsumerGroupsInput,
        GetStreamConsumersInput, GetStreamPendingEntriesInput, HashEntry, ImportKeysInput,
        KeyInfoInput, KeyValue, ListVectorSetElementsInput, PublishPubSubInput, RedisValue,
        RenameKeyInput, ScanKeysInput, SearchArrayInput, SearchFieldType, SearchIndexFieldInput,
        SearchIndexInput, SearchKeyType, SearchQueryInput, SelectDatabaseInput,
        SetArrayElementInput, SetJsonPathInput, SetKeyInput, SetKeyTtlInput,
        SetVectorSetAttributesInput, SortedSetEntry, StopPubSubInput, StreamEntry, StreamField,
        UpdateSlowLogConfigInput, VectorSetElementInput, VectorSetElementPayload,
        VectorSetKeyInput, VectorSimilarityQueryInput,
    },
    error::AppError,
    persistence::{ConnectionSecrets, ProfileRepository, SecretStore},
    redis::{RedisOperations, RedisService},
};

struct TestProfiles {
    profiles: Vec<ConnectionProfile>,
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "设置 REDIX_TEST_REDIS_STACK_URL 后用 cargo test -- --ignored --nocapture 运行"]
async fn redis_stack_json_path_flow_when_redis_stack_is_available() {
    let Ok(url) = std::env::var("REDIX_TEST_REDIS_STACK_URL") else {
        eprintln!("skipped: REDIX_TEST_REDIS_STACK_URL is not set");
        return;
    };
    let (profile, password) = integration_profile(&url);
    let secrets = TestSecrets::default();
    if let Some(password) = password.as_deref() {
        secrets
            .write(
                "integration",
                &ConnectionSecrets {
                    password: Some(password.to_owned()),
                    ..ConnectionSecrets::default()
                },
            )
            .unwrap();
    }
    let service = RedisService::new(
        std::sync::Arc::new(TestProfiles {
            profiles: vec![profile],
        }),
        std::sync::Arc::new(secrets),
    );
    service.open_connection("integration").await.unwrap();
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock must be after Unix epoch")
        .as_nanos();
    let key = format!("redix:json-path:{}:{timestamp}", std::process::id());

    let flow = async {
        let capabilities = service
            .get_module_capabilities("integration")
            .await
            .map_err(|error| error.code().to_owned())?;
        if !capabilities.json_supported {
            eprintln!("skipped: RedisJSON module is not installed");
            return Ok(());
        }

        let created = service
            .set_json_path(SetJsonPathInput {
                connection_id: "integration".into(),
                key: key.clone(),
                path: "$".into(),
                value: serde_json::json!({
                    "profile": {
                        "name": "redix",
                        "active": true
                    },
                    "items": [1, 2]
                }),
            })
            .await
            .map_err(|error| error.code().to_owned())?;
        if created.affected != 1 || created.key != key || created.ttl_ms != -1 {
            return Err("JSON.SET root did not report the expected mutation".into());
        }

        let name = service
            .get_json_path(GetJsonPathInput {
                connection_id: "integration".into(),
                key: key.clone(),
                path: "$.profile.name".into(),
            })
            .await
            .map_err(|error| error.code().to_owned())?;
        if name.value != Some(serde_json::json!("redix")) {
            return Err("JSON.GET nested path returned an unexpected value".into());
        }

        service
            .set_json_path(SetJsonPathInput {
                connection_id: "integration".into(),
                key: key.clone(),
                path: "$.profile.name".into(),
                value: serde_json::json!("codex"),
            })
            .await
            .map_err(|error| error.code().to_owned())?;
        let appended = service
            .append_json_array(AppendJsonArrayInput {
                connection_id: "integration".into(),
                key: key.clone(),
                path: "$.items".into(),
                values: vec![serde_json::json!(3), serde_json::json!(4)],
            })
            .await
            .map_err(|error| error.code().to_owned())?;
        if appended.new_length != Some(4) {
            return Err("JSON.ARRAPPEND did not report the new array length".into());
        }

        let items = service
            .get_json_path(GetJsonPathInput {
                connection_id: "integration".into(),
                key: key.clone(),
                path: "$.items".into(),
            })
            .await
            .map_err(|error| error.code().to_owned())?;
        if items.value != Some(serde_json::json!([1, 2, 3, 4])) {
            return Err("JSON.GET array path returned an unexpected value".into());
        }

        let deleted = service
            .delete_json_path(DeleteJsonPathInput {
                connection_id: "integration".into(),
                key: key.clone(),
                path: "$.profile.active".into(),
            })
            .await
            .map_err(|error| error.code().to_owned())?;
        if deleted.affected != 1 {
            return Err("JSON.DEL did not report one deleted path".into());
        }
        let missing = service
            .get_json_path(GetJsonPathInput {
                connection_id: "integration".into(),
                key: key.clone(),
                path: "$.profile.active".into(),
            })
            .await
            .map_err(|error| error.code().to_owned())?;
        if missing.value.is_some() {
            return Err("deleted JSON path is still readable".into());
        }

        Ok::<(), String>(())
    }
    .await;

    let cleanup = async {
        service
            .delete_key("integration", &key)
            .await
            .map_err(|error| error.code().to_owned())?;
        service
            .close_connection("integration")
            .await
            .map_err(|error| error.code().to_owned())?;
        Ok::<(), String>(())
    }
    .await;

    match (flow, cleanup) {
        (Ok(()), Ok(())) => {}
        (Err(flow), Ok(())) => panic!("Redis Stack JSON path flow failed: {flow}"),
        (Ok(()), Err(cleanup)) => panic!("Redis Stack JSON path cleanup failed: {cleanup}"),
        (Err(flow), Err(cleanup)) => {
            panic!("Redis Stack JSON path flow failed: {flow}; cleanup also failed: {cleanup}")
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "设置 REDIX_TEST_REDIS_STACK_URL 后用 cargo test -- --ignored --nocapture 运行"]
async fn redis_stack_search_flow_when_redis_stack_is_available() {
    let Ok(url) = std::env::var("REDIX_TEST_REDIS_STACK_URL") else {
        eprintln!("skipped: REDIX_TEST_REDIS_STACK_URL is not set");
        return;
    };
    let (profile, password) = integration_profile(&url);
    let secrets = TestSecrets::default();
    if let Some(password) = password.as_deref() {
        secrets
            .write(
                "integration",
                &ConnectionSecrets {
                    password: Some(password.to_owned()),
                    ..ConnectionSecrets::default()
                },
            )
            .unwrap();
    }
    let service = RedisService::new(
        std::sync::Arc::new(TestProfiles {
            profiles: vec![profile],
        }),
        std::sync::Arc::new(secrets),
    );
    service.open_connection("integration").await.unwrap();
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock must be after Unix epoch")
        .as_nanos();
    let prefix = format!("redix:search:test:{}:{timestamp}:", std::process::id());
    let key = format!("{prefix}one");
    let index = format!("redix_search_{}_{}", std::process::id(), timestamp);

    let flow = async {
        let capabilities = service
            .get_module_capabilities("integration")
            .await
            .map_err(|error| error.code().to_owned())?;
        if !capabilities.search_compatible() {
            eprintln!("skipped: RedisSearch module is not installed or is too old");
            return Ok::<(), String>(());
        }

        service
            .set_key(SetKeyInput {
                connection_id: "integration".into(),
                key: key.clone(),
                value: RedisValue::Hash {
                    fields: vec![
                        HashEntry {
                            field: "title".into(),
                            value: "hello world".into(),
                        },
                        HashEntry {
                            field: "label".into(),
                            value: "blue".into(),
                        },
                    ],
                },
            })
            .await
            .map_err(|error| error.code().to_owned())?;

        service
            .create_search_index(CreateSearchIndexInput {
                connection_id: "integration".into(),
                index: index.clone(),
                key_type: SearchKeyType::Hash,
                prefixes: vec![prefix.clone()],
                fields: vec![
                    SearchIndexFieldInput {
                        name: "title".into(),
                        field_type: SearchFieldType::Text,
                    },
                    SearchIndexFieldInput {
                        name: "label".into(),
                        field_type: SearchFieldType::Tag,
                    },
                ],
            })
            .await
            .map_err(|error| error.code().to_owned())?;

        let indexes = service
            .list_search_indexes("integration")
            .await
            .map_err(|error| error.code().to_owned())?;
        if !indexes.indexes.iter().any(|item| item.name == index) {
            return Err("created RedisSearch index is not listed".into());
        }

        let info = service
            .get_search_index(SearchIndexInput {
                connection_id: "integration".into(),
                index: index.clone(),
            })
            .await
            .map_err(|error| error.code().to_owned())?;
        if info.key_type != "HASH" || info.prefixes != vec![prefix.clone()] {
            return Err("RedisSearch INFO returned an unexpected definition".into());
        }

        let result = service
            .search_keys(SearchQueryInput {
                connection_id: "integration".into(),
                index: index.clone(),
                query: "hello".into(),
                offset: 0,
                limit: 20,
                include_content: false,
            })
            .await
            .map_err(|error| error.code().to_owned())?;
        if result.total != 1
            || result.keys.len() != 1
            || result.keys[0].key != key
            || result.keys[0].key_type != "hash"
        {
            return Err("RedisSearch query returned an unexpected key result".into());
        }

        let associations = service
            .get_key_search_indexes(GetKeySearchIndexesInput {
                connection_id: "integration".into(),
                key: key.clone(),
            })
            .await
            .map_err(|error| error.code().to_owned())?;
        if !associations.iter().any(|item| item.name == index) {
            return Err("Browser key association did not include the created index".into());
        }

        service
            .delete_search_index(SearchIndexInput {
                connection_id: "integration".into(),
                index: index.clone(),
            })
            .await
            .map_err(|error| error.code().to_owned())?;
        service
            .get_key("integration", &key)
            .await
            .map_err(|error| error.code().to_owned())?;
        Ok::<(), String>(())
    }
    .await;

    let cleanup = async {
        service
            .delete_key("integration", &key)
            .await
            .map_err(|error| error.code().to_owned())?;
        service
            .delete_search_index(SearchIndexInput {
                connection_id: "integration".into(),
                index: index.clone(),
            })
            .await
            .ok();
        service
            .close_connection("integration")
            .await
            .map_err(|error| error.code().to_owned())?;
        Ok::<(), String>(())
    }
    .await;

    match (flow, cleanup) {
        (Ok(()), Ok(())) => {}
        (Err(flow), Ok(())) => panic!("Redis Stack Search flow failed: {flow}"),
        (Ok(()), Err(cleanup)) => panic!("Redis Stack Search cleanup failed: {cleanup}"),
        (Err(flow), Err(cleanup)) => {
            panic!("Redis Stack Search flow failed: {flow}; cleanup also failed: {cleanup}")
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "设置 REDIX_TEST_REDIS_STACK_URL 后用 cargo test -- --ignored --nocapture 运行"]
async fn redis_stack_array_and_vector_set_flow_when_redis_stack_is_available() {
    let Ok(url) = std::env::var("REDIX_TEST_REDIS_STACK_URL") else {
        eprintln!("skipped: REDIX_TEST_REDIS_STACK_URL is not set");
        return;
    };
    let (profile, password) = integration_profile(&url);
    let secrets = TestSecrets::default();
    if let Some(password) = password.as_deref() {
        secrets
            .write(
                "integration",
                &ConnectionSecrets {
                    password: Some(password.to_owned()),
                    ..ConnectionSecrets::default()
                },
            )
            .unwrap();
    }
    let service = RedisService::new(
        std::sync::Arc::new(TestProfiles {
            profiles: vec![profile],
        }),
        std::sync::Arc::new(secrets),
    );
    service.open_connection("integration").await.unwrap();
    let suffix = format!(
        "{}:{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock must be after Unix epoch")
            .as_nanos()
    );
    let array_key = format!("redix:array:integration:{suffix}");
    let vector_key = format!("redix:vector-set:integration:{suffix}");

    let flow = async {
        let capabilities = service
            .get_module_capabilities("integration")
            .await
            .map_err(|error| error.code().to_owned())?;

        if capabilities.array_supported {
            let created = service
                .create_array(CreateArrayInput {
                    connection_id: "integration".into(),
                    key: array_key.clone(),
                    mode: ArrayCreateMode::Contiguous,
                    start_index: Some("0".into()),
                    values: vec!["one".into(), "two".into()],
                    elements: Vec::new(),
                    ttl_ms: None,
                })
                .await
                .map_err(|error| error.code().to_owned())?;
            if created.key != array_key {
                return Err("Array create returned an unexpected key".to_owned());
            }

            let summary = service
                .get_array_summary(redix_lib::domain::ArrayKeyInput {
                    connection_id: "integration".into(),
                    key: array_key.clone(),
                })
                .await
                .map_err(|error| error.code().to_owned())?;
            if summary.length != "2" || summary.count != "2" {
                return Err("Array summary returned unexpected length or count".to_owned());
            }

            let range = service
                .get_array_range(ArrayRangeInput {
                    connection_id: "integration".into(),
                    key: array_key.clone(),
                    start: "0".into(),
                    end: "1".into(),
                })
                .await
                .map_err(|error| error.code().to_owned())?;
            if range.cells.len() != 2
                || range.cells[0].value.as_deref() != Some("one")
                || range.cells[1].value.as_deref() != Some("two")
            {
                return Err("Array range returned unexpected cells".to_owned());
            }

            service
                .set_array_element(SetArrayElementInput {
                    connection_id: "integration".into(),
                    key: array_key.clone(),
                    index: "1".into(),
                    value: "updated".into(),
                })
                .await
                .map_err(|error| error.code().to_owned())?;
            service
                .append_array_elements(AppendArrayInput {
                    connection_id: "integration".into(),
                    key: array_key.clone(),
                    values: vec!["three".into()],
                })
                .await
                .map_err(|error| error.code().to_owned())?;
            let used = service
                .aggregate_array(AggregateArrayInput {
                    connection_id: "integration".into(),
                    key: array_key.clone(),
                    operation: ArrayAggregateOperation::Used,
                    start: None,
                    end: None,
                    values: Vec::new(),
                    limit: 50,
                })
                .await
                .map_err(|error| error.code().to_owned())?;
            if used.value != "3" {
                return Err("Array USED aggregate returned an unexpected value".to_owned());
            }

            let search = service
                .search_array(SearchArrayInput {
                    connection_id: "integration".into(),
                    key: array_key.clone(),
                    start: None,
                    end: None,
                    predicates: vec![ArrayPredicate {
                        criteria: "EXACT".into(),
                        value: "updated".into(),
                    }],
                    combinator: None,
                    nocase: false,
                    with_values: true,
                    limit: 50,
                })
                .await
                .map_err(|error| error.code().to_owned())?;
            if search.elements.len() != 1 || search.elements[0].index != "1" {
                return Err("Array search returned an unexpected match".to_owned());
            }

            service
                .delete_array_elements(DeleteArrayElementsInput {
                    connection_id: "integration".into(),
                    key: array_key.clone(),
                    indices: vec!["1".into()],
                })
                .await
                .map_err(|error| error.code().to_owned())?;
            service
                .delete_array_range(DeleteArrayRangeInput {
                    connection_id: "integration".into(),
                    key: array_key.clone(),
                    start: "0".into(),
                    end: "0".into(),
                })
                .await
                .map_err(|error| error.code().to_owned())?;
        } else {
            eprintln!("skipped: Redis Array module is not installed");
        }

        if capabilities.vector_set_supported {
            let created = service
                .create_vector_set(CreateVectorSetInput {
                    connection_id: "integration".into(),
                    key: vector_key.clone(),
                    dimension: 3,
                    quantization: None,
                    elements: vec![
                        VectorSetElementPayload {
                            name: "one".into(),
                            vector_values: Some(vec![1.0, 0.0, 0.0]),
                            vector_fp32_base64: None,
                            attributes: Some(serde_json::json!({"kind": "seed"})),
                        },
                        VectorSetElementPayload {
                            name: "two".into(),
                            vector_values: Some(vec![0.0, 1.0, 0.0]),
                            vector_fp32_base64: None,
                            attributes: None,
                        },
                    ],
                    ttl_ms: None,
                })
                .await
                .map_err(|error| error.code().to_owned())?;
            if created.key != vector_key {
                return Err("Vector Set create returned an unexpected key".to_owned());
            }

            let summary = service
                .get_vector_set_summary(VectorSetKeyInput {
                    connection_id: "integration".into(),
                    key: vector_key.clone(),
                })
                .await
                .map_err(|error| error.code().to_owned())?;
            if summary.total != "2" || summary.dimension != Some(3) {
                return Err("Vector Set summary returned unexpected metadata".to_owned());
            }

            let page = service
                .list_vector_set_elements(ListVectorSetElementsInput {
                    connection_id: "integration".into(),
                    key: vector_key.clone(),
                    start: None,
                    end: None,
                    limit: 2,
                })
                .await
                .map_err(|error| error.code().to_owned())?;
            if page.elements.len() != 2 {
                return Err(
                    "Vector Set listing returned an unexpected number of elements".to_owned(),
                );
            }

            let element = service
                .get_vector_set_element(VectorSetElementInput {
                    connection_id: "integration".into(),
                    key: vector_key.clone(),
                    element: "one".into(),
                })
                .await
                .map_err(|error| error.code().to_owned())?;
            if element.vector_base64.is_none()
                || element.attributes != Some(serde_json::json!({"kind": "seed"}))
            {
                return Err("Vector Set element returned unexpected data".to_owned());
            }

            let updated = service
                .set_vector_set_attributes(SetVectorSetAttributesInput {
                    connection_id: "integration".into(),
                    key: vector_key.clone(),
                    element: "one".into(),
                    attributes: serde_json::json!({"kind": "updated"}),
                })
                .await
                .map_err(|error| error.code().to_owned())?;
            if updated.attributes != Some(serde_json::json!({"kind": "updated"})) {
                return Err("Vector Set attributes were not updated".to_owned());
            }

            let matches = service
                .search_vector_set(VectorSimilarityQueryInput {
                    connection_id: "integration".into(),
                    key: vector_key.clone(),
                    by_element: Some("one".into()),
                    by_vector: None,
                    by_vector_base64: None,
                    count: 2,
                    with_attributes: true,
                })
                .await
                .map_err(|error| error.code().to_owned())?;
            if matches.matches.is_empty() {
                return Err("Vector Set similarity search returned no matches".to_owned());
            }

            let embedding = service
                .download_vector_embedding(VectorSetElementInput {
                    connection_id: "integration".into(),
                    key: vector_key.clone(),
                    element: "one".into(),
                })
                .await
                .map_err(|error| error.code().to_owned())?;
            if embedding.is_empty() {
                return Err("Vector Set embedding download returned an empty value".to_owned());
            }

            service
                .delete_vector_set_attributes(VectorSetElementInput {
                    connection_id: "integration".into(),
                    key: vector_key.clone(),
                    element: "one".into(),
                })
                .await
                .map_err(|error| error.code().to_owned())?;
            let deleted = service
                .delete_vector_set_elements(DeleteVectorSetElementsInput {
                    connection_id: "integration".into(),
                    key: vector_key.clone(),
                    elements: vec!["two".into()],
                })
                .await
                .map_err(|error| error.code().to_owned())?;
            if deleted != 1 {
                return Err("Vector Set delete returned an unexpected count".to_owned());
            }
        } else {
            eprintln!("skipped: Redis Vector Set module is not installed");
        }

        if !capabilities.array_supported && !capabilities.vector_set_supported {
            eprintln!("skipped: Array and Vector Set commands are unavailable");
        }
        Ok::<(), String>(())
    }
    .await;

    let cleanup = async {
        let mut errors = Vec::new();
        if let Err(error) = service.delete_key("integration", &array_key).await {
            errors.push(format!("Array cleanup: {}", error.code()));
        }
        if let Err(error) = service.delete_key("integration", &vector_key).await {
            errors.push(format!("Vector Set cleanup: {}", error.code()));
        }
        if let Err(error) = service.close_connection("integration").await {
            errors.push(format!("connection cleanup: {}", error.code()));
        }
        if errors.is_empty() {
            Ok::<(), String>(())
        } else {
            Err(errors.join("; "))
        }
    }
    .await;

    match (flow, cleanup) {
        (Ok(()), Ok(())) => {}
        (Err(flow), Ok(())) => panic!("Redis Stack Array/Vector Set flow failed: {flow}"),
        (Ok(()), Err(cleanup)) => panic!("Redis Stack Array/Vector Set cleanup failed: {cleanup}"),
        (Err(flow), Err(cleanup)) => {
            panic!(
                "Redis Stack Array/Vector Set flow failed: {flow}; cleanup also failed: {cleanup}"
            )
        }
    }
}

impl ProfileRepository for TestProfiles {
    fn load(&self) -> Result<Vec<ConnectionProfile>, AppError> {
        Ok(self.profiles.clone())
    }

    fn save(&self, _profiles: &[ConnectionProfile]) -> Result<(), AppError> {
        Ok(())
    }
}

struct FailingSaveProfiles {
    profile: ConnectionProfile,
}

impl ProfileRepository for FailingSaveProfiles {
    fn load(&self) -> Result<Vec<ConnectionProfile>, AppError> {
        Ok(vec![self.profile.clone()])
    }

    fn save(&self, _profiles: &[ConnectionProfile]) -> Result<(), AppError> {
        Err(AppError::PersistenceFailed)
    }
}

#[derive(Default)]
struct TestSecrets {
    values: Mutex<HashMap<String, ConnectionSecrets>>,
}

impl SecretStore for TestSecrets {
    fn read(&self, connection_id: &str) -> Result<Option<ConnectionSecrets>, AppError> {
        Ok(self
            .values
            .lock()
            .expect("test secret lock must not be poisoned")
            .get(connection_id)
            .cloned())
    }

    fn write(&self, connection_id: &str, secrets: &ConnectionSecrets) -> Result<(), AppError> {
        self.values
            .lock()
            .expect("test secret lock must not be poisoned")
            .insert(connection_id.to_owned(), secrets.clone());
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
        ssh: None,
        sentinel: None,
        cluster: None,
        id: "integration".into(),
        name: "Integration".into(),
        host,
        port,
        username: info.redis_settings().username().map(str::to_owned),
        database: info.redis_settings().db() as u8,
        has_password: password.is_some(),
        tls: false,
        verify_server_cert: true,
        ca_certificate_name: None,
        client_certificate_name: None,
        has_ca_certificate: false,
        has_client_certificate: false,
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

    let instance = service
        .get_instance_overview("integration")
        .await
        .map_err(|error| error.code().to_owned())?;
    if instance.server_version.is_none() && instance.connected_clients.is_none() {
        return Err("instance overview did not return any server metric".into());
    }
    service
        .select_database(SelectDatabaseInput {
            connection_id: "integration".into(),
            database: 0,
        })
        .await
        .map_err(|error| error.code().to_owned())?;

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

    let databases = service
        .get_database_overview("integration")
        .await
        .map_err(|error| error.code().to_owned())?;
    if !databases
        .iter()
        .any(|database| database.key_count.is_some())
    {
        return Err("database overview did not return a key count".into());
    }

    let database_one_key = format!("{}:database-one", keys.prefix);
    service
        .select_database(SelectDatabaseInput {
            connection_id: "integration".into(),
            database: 1,
        })
        .await
        .map_err(|error| error.code().to_owned())?;
    service
        .set_key(SetKeyInput {
            connection_id: "integration".into(),
            key: database_one_key.clone(),
            value: RedisValue::String {
                value: "database-one".into(),
            },
        })
        .await
        .map_err(|error| error.code().to_owned())?;
    service
        .select_database(SelectDatabaseInput {
            connection_id: "integration".into(),
            database: 0,
        })
        .await
        .map_err(|error| error.code().to_owned())?;
    if service
        .get_key("integration", &database_one_key)
        .await
        .is_ok()
    {
        return Err("database selection did not isolate database one".into());
    }
    service
        .select_database(SelectDatabaseInput {
            connection_id: "integration".into(),
            database: 1,
        })
        .await
        .map_err(|error| error.code().to_owned())?;
    service
        .delete_key("integration", &database_one_key)
        .await
        .map_err(|error| error.code().to_owned())?;
    service
        .select_database(SelectDatabaseInput {
            connection_id: "integration".into(),
            database: 0,
        })
        .await
        .map_err(|error| error.code().to_owned())?;

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
        if !matches!(
            service.get_key("integration", key).await,
            Err(AppError::KeyNotFound)
        ) {
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
            Err(error) if error.code() == "KEY_NOT_FOUND" => {}
            Err(error) => {
                return Err(format!(
                    "deleted key returned {} instead of KEY_NOT_FOUND",
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

struct AnalysisKeys {
    pattern: String,
    keys: Vec<String>,
    string_keys: Vec<String>,
    hash: String,
    list: String,
    stream: String,
}

impl AnalysisKeys {
    fn for_prefix(prefix: &str) -> Self {
        let pattern = format!("{prefix}:analysis:*");
        let string_keys: Vec<_> = (0..498)
            .map(|index| format!("{prefix}:analysis:string:{index}"))
            .collect();
        let hash = format!("{prefix}:analysis:hash");
        let list = format!("{prefix}:analysis:list");
        let stream = format!("{prefix}:analysis:stream");
        let mut keys = string_keys.clone();
        keys.extend([hash.clone(), list.clone(), stream.clone()]);
        Self {
            pattern,
            keys,
            string_keys,
            hash,
            list,
            stream,
        }
    }
}

async fn cleanup_analysis_keys(service: &RedisService, keys: &AnalysisKeys) -> Result<(), String> {
    let mut failures = Vec::new();
    for batch in keys.keys.chunks(500) {
        match service
            .delete_keys(DeleteKeysInput {
                connection_id: "integration".into(),
                keys: batch.to_vec(),
            })
            .await
        {
            Ok(deleted) if deleted == batch.len() as u64 => {}
            Ok(deleted) => failures.push(format!(
                "DEL deleted {deleted} keys instead of {}",
                batch.len()
            )),
            Err(error) => failures.push(error.code().to_owned()),
        }
    }
    if let Err(error) = service.close_connection("integration").await {
        failures.push(error.code().to_owned());
    }

    if failures.is_empty() {
        Ok(())
    } else {
        Err(format!("analysis cleanup failed: {}", failures.join(", ")))
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "设置 REDIX_TEST_REDIS_URL 后用 cargo test -- --ignored --nocapture 运行"]
async fn analyzes_database_details_and_metadata_batches_when_redis_is_available() {
    let url = std::env::var("REDIX_TEST_REDIS_URL")
        .expect("请设置 REDIX_TEST_REDIS_URL 后运行 Redis 集成测试");
    let (profile, password) = integration_profile(&url);
    let secrets = TestSecrets::default();
    if let Some(password) = password.as_deref() {
        secrets
            .write(
                "integration",
                &ConnectionSecrets {
                    password: Some(password.to_owned()),
                    ..ConnectionSecrets::default()
                },
            )
            .unwrap();
    }
    let service = RedisService::new(
        std::sync::Arc::new(TestProfiles {
            profiles: vec![profile],
        }),
        std::sync::Arc::new(secrets),
    );
    let unique = TestKeys::unique();
    let keys = AnalysisKeys::for_prefix(&unique.prefix);
    service.open_connection("integration").await.unwrap();

    let flow = async {
        let details = service
            .get_instance_details("integration")
            .await
            .map_err(|error| error.code().to_owned())?;
        if details.overview.server_version.is_none() {
            return Err("instance details did not include the Redis version".into());
        }

        for key in &keys.string_keys {
            service
                .create_key(CreateKeyInput {
                    connection_id: "integration".into(),
                    key: key.clone(),
                    value: RedisValue::String {
                        value: "analysis".into(),
                    },
                    ttl_ms: None,
                })
                .await
                .map_err(|error| error.code().to_owned())?;
        }
        for (key, value) in [
            (
                &keys.hash,
                RedisValue::Hash {
                    fields: vec![HashEntry {
                        field: "field".into(),
                        value: "value".into(),
                    }],
                },
            ),
            (
                &keys.list,
                RedisValue::List {
                    items: vec!["first".into(), "second".into()],
                },
            ),
            (
                &keys.stream,
                RedisValue::Stream {
                    entries: vec![StreamEntry {
                        id: "1-0".into(),
                        fields: vec![StreamField {
                            field: "event".into(),
                            value: "analysis".into(),
                        }],
                    }],
                },
            ),
        ] {
            service
                .create_key(CreateKeyInput {
                    connection_id: "integration".into(),
                    key: key.clone(),
                    value,
                    ttl_ms: None,
                })
                .await
                .map_err(|error| error.code().to_owned())?;
        }

        let report = service
            .analyze_database(AnalyzeDatabaseInput {
                connection_id: "integration".into(),
                pattern: keys.pattern.clone(),
                delimiter: ":".into(),
                max_keys: 1_000,
            })
            .await
            .map_err(|error| error.code().to_owned())?;
        if report.pattern != keys.pattern
            || report.progress.processed != 501
            || report.total_keys.total != 501
            || report.progress.truncated
        {
            return Err("database analysis did not report all matching keys".into());
        }
        if !report
            .top_namespaces_by_keys
            .iter()
            .any(|namespace| namespace.namespace == "redix" && namespace.keys == 501)
        {
            return Err("database analysis did not aggregate the redix namespace".into());
        }
        for expected_type in ["string", "hash", "list", "stream"] {
            if !report
                .total_keys
                .types
                .iter()
                .any(|summary| summary.r#type == expected_type)
            {
                return Err(format!(
                    "database analysis did not include {expected_type} keys"
                ));
            }
        }
        Ok::<(), String>(())
    }
    .await;
    let cleanup = cleanup_analysis_keys(&service, &keys).await;

    match (flow, cleanup) {
        (Ok(()), Ok(())) => {}
        (Err(flow), Ok(())) => panic!("database analysis integration flow failed: {flow}"),
        (Ok(()), Err(cleanup)) => panic!("database analysis cleanup failed: {cleanup}"),
        (Err(flow), Err(cleanup)) => {
            panic!(
                "database analysis integration flow failed: {flow}; cleanup also failed: {cleanup}"
            )
        }
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
        secrets
            .write(
                "integration",
                &ConnectionSecrets {
                    password: Some(password.to_owned()),
                    ..ConnectionSecrets::default()
                },
            )
            .unwrap();
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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "设置 REDIX_TEST_REDIS_URL 后用 cargo test -- --ignored --nocapture 运行"]
async fn preserves_active_client_when_database_profile_save_fails() {
    let url = std::env::var("REDIX_TEST_REDIS_URL")
        .expect("请设置 REDIX_TEST_REDIS_URL 后运行 Redis 集成测试");
    let (profile, password) = integration_profile(&url);
    let profiles = std::sync::Arc::new(FailingSaveProfiles {
        profile: profile.clone(),
    });
    let secrets = TestSecrets::default();
    if let Some(password) = password.as_deref() {
        secrets
            .write(
                "integration",
                &ConnectionSecrets {
                    password: Some(password.to_owned()),
                    ..ConnectionSecrets::default()
                },
            )
            .unwrap();
    }
    let service = RedisService::new(profiles.clone(), std::sync::Arc::new(secrets));

    service.open_connection("integration").await.unwrap();
    let target_database = if profile.database == 0 { 1 } else { 0 };
    assert_eq!(
        service
            .select_database(SelectDatabaseInput {
                connection_id: "integration".into(),
                database: target_database,
            })
            .await
            .unwrap_err(),
        AppError::PersistenceFailed
    );
    assert_eq!(profiles.load().unwrap()[0].database, profile.database);

    let ping = service
        .execute_command("integration", "PING")
        .await
        .expect("the old active client must remain usable");
    assert_eq!(ping.value, serde_json::json!("PONG"));
    service.close_connection("integration").await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "设置 REDIX_TEST_REDIS_URL 后用 cargo test -- --ignored --nocapture 运行"]
async fn runs_slow_log_and_pubsub_flow_when_redis_is_available() {
    let url = std::env::var("REDIX_TEST_REDIS_URL")
        .expect("请设置 REDIX_TEST_REDIS_URL 后运行 Redis 集成测试");
    let (profile, password) = integration_profile(&url);
    let secrets = TestSecrets::default();
    if let Some(password) = password.as_deref() {
        secrets
            .write(
                "integration",
                &ConnectionSecrets {
                    password: Some(password.to_owned()),
                    ..ConnectionSecrets::default()
                },
            )
            .unwrap();
    }
    let service = RedisService::new(
        std::sync::Arc::new(TestProfiles {
            profiles: vec![profile],
        }),
        std::sync::Arc::new(secrets),
    );

    service.open_connection("integration").await.unwrap();
    service.clear_slow_logs("integration").await.unwrap();
    let config = service.get_slow_log_config("integration").await.unwrap();
    let updated = service
        .update_slow_log_config(UpdateSlowLogConfigInput {
            connection_id: "integration".into(),
            slowlog_max_len: Some(config.slowlog_max_len),
            slowlog_log_slower_than: Some(config.slowlog_log_slower_than),
        })
        .await
        .unwrap();
    assert_eq!(updated, config);

    let logs = service
        .get_slow_logs(GetSlowLogsInput {
            connection_id: "integration".into(),
            count: 20,
        })
        .await
        .unwrap();
    assert!(logs.iter().all(|entry| !entry.args.is_empty()));

    let _receivers = service
        .publish_pub_sub(PublishPubSubInput {
            connection_id: "integration".into(),
            channel: "redix:integration".into(),
            message: "hello from redix".into(),
        })
        .await
        .unwrap();

    service
        .stop_pub_sub(StopPubSubInput {
            connection_id: "integration".into(),
            session_id: "missing-session".into(),
        })
        .await
        .unwrap();
    service.close_connection("integration").await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "设置 REDIX_TEST_REDIS_URL 后用 cargo test -- --ignored --nocapture 运行"]
async fn runs_stream_consumer_group_flow_when_redis_is_available() {
    let url = std::env::var("REDIX_TEST_REDIS_URL")
        .expect("请设置 REDIX_TEST_REDIS_URL 后运行 Redis 集成测试");
    let (profile, password) = integration_profile(&url);
    let secrets = TestSecrets::default();
    if let Some(password) = password.as_deref() {
        secrets
            .write(
                "integration",
                &ConnectionSecrets {
                    password: Some(password.to_owned()),
                    ..ConnectionSecrets::default()
                },
            )
            .unwrap();
    }
    let service = RedisService::new(
        std::sync::Arc::new(TestProfiles {
            profiles: vec![profile],
        }),
        std::sync::Arc::new(secrets),
    );
    let keys = TestKeys::unique();
    service.open_connection("integration").await.unwrap();

    let flow = async {
        service
            .create_key(CreateKeyInput {
                connection_id: "integration".into(),
                key: keys.stream.clone(),
                value: RedisValue::Stream {
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
                },
                ttl_ms: None,
            })
            .await
            .map_err(|error| error.code().to_owned())?;
        service
            .create_stream_consumer_group(CreateStreamConsumerGroupInput {
                connection_id: "integration".into(),
                key: keys.stream.clone(),
                name: "workers".into(),
                last_delivered_id: "0-0".into(),
            })
            .await
            .map_err(|error| error.code().to_owned())?;

        let groups = service
            .get_stream_consumer_groups(GetStreamConsumerGroupsInput {
                connection_id: "integration".into(),
                key: keys.stream.clone(),
            })
            .await
            .map_err(|error| error.code().to_owned())?;
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].name, "workers");
        assert_eq!(groups[0].pending, 0);

        let consumers = service
            .get_stream_consumers(GetStreamConsumersInput {
                connection_id: "integration".into(),
                key: keys.stream.clone(),
                group: "workers".into(),
            })
            .await
            .map_err(|error| error.code().to_owned())?;
        assert!(consumers.is_empty());
        let pending = service
            .get_stream_pending_entries(GetStreamPendingEntriesInput {
                connection_id: "integration".into(),
                key: keys.stream.clone(),
                group: "workers".into(),
                count: 20,
                consumer: None,
            })
            .await
            .map_err(|error| error.code().to_owned())?;
        assert!(pending.is_empty());

        let raw_client = redis::Client::open(url.as_str()).unwrap();
        let mut raw_connection = raw_client.get_multiplexed_async_connection().await.unwrap();
        let _: redis::Value = redis::cmd("XREADGROUP")
            .arg("GROUP")
            .arg("workers")
            .arg("consumer-1")
            .arg("COUNT")
            .arg(1)
            .arg("STREAMS")
            .arg(&keys.stream)
            .arg(">")
            .query_async(&mut raw_connection)
            .await
            .unwrap();

        let consumers = service
            .get_stream_consumers(GetStreamConsumersInput {
                connection_id: "integration".into(),
                key: keys.stream.clone(),
                group: "workers".into(),
            })
            .await
            .map_err(|error| error.code().to_owned())?;
        assert_eq!(consumers.len(), 1);
        assert_eq!(consumers[0].name, "consumer-1");
        assert_eq!(consumers[0].pending, 1);

        let pending = service
            .get_stream_pending_entries(GetStreamPendingEntriesInput {
                connection_id: "integration".into(),
                key: keys.stream.clone(),
                group: "workers".into(),
                count: 20,
                consumer: Some("consumer-1".into()),
            })
            .await
            .map_err(|error| error.code().to_owned())?;
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].consumer, "consumer-1");
        assert_eq!(pending[0].deliveries, 1);

        let claim_input = redix_lib::domain::ClaimStreamPendingEntriesInput {
            connection_id: "integration".into(),
            key: keys.stream.clone(),
            group: "workers".into(),
            consumer: "replacement".into(),
            min_idle_ms: 0,
            entries: pending.iter().map(|entry| entry.id.clone()).collect(),
        };
        let claimed = service
            .claim_stream_pending_entries(claim_input.clone())
            .await
            .map_err(|error| error.code().to_owned())?;
        if claimed.len() != pending.len() {
            return Err("XCLAIM did not return selected IDs".into());
        }
        let owned = service
            .get_stream_pending_entries(GetStreamPendingEntriesInput {
                connection_id: "integration".into(),
                key: keys.stream.clone(),
                group: "workers".into(),
                count: 100,
                consumer: Some("replacement".into()),
            })
            .await
            .map_err(|error| error.code().to_owned())?;
        if owned.len() != pending.len() || owned.iter().any(|entry| entry.consumer != "replacement")
        {
            return Err("XCLAIM did not transfer pending ownership".into());
        }
        let skipped = service
            .claim_stream_pending_entries(redix_lib::domain::ClaimStreamPendingEntriesInput {
                min_idle_ms: 9_007_199_254_740_991,
                ..claim_input
            })
            .await
            .map_err(|error| error.code().to_owned())?;
        if !skipped.is_empty() {
            return Err("XCLAIM ignored min-idle-time".into());
        }

        let acknowledged = service
            .acknowledge_stream_pending_entries(AcknowledgeStreamPendingEntriesInput {
                connection_id: "integration".into(),
                key: keys.stream.clone(),
                group: "workers".into(),
                entries: vec![pending[0].id.clone()],
            })
            .await
            .map_err(|error| error.code().to_owned())?;
        assert_eq!(acknowledged, 1);

        let removed_consumer = service
            .delete_stream_consumer(DeleteStreamConsumerInput {
                connection_id: "integration".into(),
                key: keys.stream.clone(),
                group: "workers".into(),
                consumer: "consumer-1".into(),
            })
            .await
            .map_err(|error| error.code().to_owned())?;
        assert_eq!(removed_consumer, 0);

        let deleted_group = service
            .delete_stream_consumer_group(DeleteStreamConsumerGroupInput {
                connection_id: "integration".into(),
                key: keys.stream.clone(),
                name: "workers".into(),
            })
            .await
            .map_err(|error| error.code().to_owned())?;
        assert_eq!(deleted_group, 1);
        Ok::<(), String>(())
    }
    .await;

    let cleanup = service.delete_key("integration", &keys.stream).await;
    let close = service.close_connection("integration").await;
    match (flow, cleanup, close) {
        (Ok(()), Ok(()), Ok(())) => {}
        (Err(flow), Ok(()), Ok(())) => panic!("Redis stream group flow failed: {flow}"),
        (flow, cleanup, close) => panic!(
            "Redis stream group flow cleanup failed: flow={:?}, cleanup={:?}, close={:?}",
            flow.map_err(|error| error),
            cleanup.map_err(|error| error.code().to_owned()),
            close.map_err(|error| error.code().to_owned())
        ),
    }
}
