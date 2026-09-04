mod support;

use redix_lib::{
    domain::{
        command_catalog, is_sensitive_command, normalize_json_path, parse_info_sections,
        parse_keyspace_line, search_version_supported, validate_json_array_append,
        validate_json_path, AcknowledgeStreamPendingEntriesInput, AppSettings,
        AppendJsonArrayInput, ArrayCreateMode, ArrayElement, ArrayRangeInput, ClusterConfig,
        ConnectionEndpoint, ConnectionProfile, ConnectionTarget, CreateArrayInput, CreateKeyInput,
        CreateSearchIndexInput, DeleteKeysInput, ExportedKey, GetJsonPathInput, GetSlowLogsInput,
        GetStreamConsumerGroupsInput, GetStreamPendingEntriesInput, ImportKeysInput, KeyInfoInput,
        ModuleCapabilities, ModuleSummary, PubSubTopic, QueryLibraryItemInput, RedisValue,
        RenameKeyInput, ScanKeysInput, SearchFieldType, SearchIndexFieldInput, SearchKeyType,
        SearchQueryInput, SelectDatabaseInput, SetJsonPathInput, StartProfilerInput,
        StartPubSubInput, StopProfilerInput, StreamEntry, StreamField, VectorSetElementPayload,
        VectorSimilarityQueryInput,
    },
    error::AppError,
};
use support::{invalid_profile, valid_profile};

#[test]
fn command_catalog_contains_safe_high_frequency_commands() {
    let names = command_catalog()
        .into_iter()
        .map(|item| item.name)
        .collect::<Vec<_>>();

    assert!(names.contains(&"PING".into()));
    assert!(names.contains(&"GET".into()));
    assert!(names.contains(&"SET".into()));
}

#[test]
fn sensitive_commands_are_not_saved_to_history() {
    assert!(is_sensitive_command("AUTH secret"));
    assert!(is_sensitive_command("CONFIG SET requirepass secret"));
    assert!(is_sensitive_command("ACL SETUSER alice on >secret"));
    assert!(!is_sensitive_command("GET user:1"));
}

#[test]
fn rejects_empty_host_zero_port_and_database_above_fifteen() {
    let profile = ConnectionProfile {
        ssh: None,
        sentinel: None,
        cluster: None,
        id: "local".into(),
        name: "Local".into(),
        host: "".into(),
        port: 0,
        username: None,
        database: 16,
        has_password: false,
        tls: false,
        verify_server_cert: true,
        ca_certificate_name: None,
        client_certificate_name: None,
        has_ca_certificate: false,
        has_client_certificate: false,
    };

    let error = profile.validate().expect_err("invalid profile must fail");
    assert_eq!(error.code(), "INVALID_CONNECTION");
}

#[test]
fn rejects_an_invalid_profile_from_the_shared_fixture() {
    let error = invalid_profile()
        .validate()
        .expect_err("invalid profile must fail");

    assert_eq!(error.code(), "INVALID_CONNECTION");
}

#[test]
fn accepts_a_valid_connection_profile() {
    assert_eq!(valid_profile().validate(), Ok(()));
}

#[test]
fn cluster_profile_is_db_zero_and_cannot_silently_mix_topologies() {
    let mut profile = valid_profile();
    profile.cluster = Some(ClusterConfig {
        nodes: vec![ConnectionEndpoint {
            host: "127.0.0.1".into(),
            port: 7000,
        }],
        read_from_replicas: false,
    });
    profile.host = "127.0.0.1".into();
    profile.port = 7000;

    assert_eq!(
        ConnectionTarget::try_from(&profile).unwrap().kind(),
        "cluster"
    );

    profile.database = 1;
    assert_eq!(profile.validate(), Err(AppError::InvalidConnection));

    profile.database = 0;
    profile.sentinel = Some(redix_lib::domain::SentinelConfig {
        master_name: "primary".into(),
        nodes: vec![ConnectionEndpoint {
            host: "127.0.0.1".into(),
            port: 26379,
        }],
        username: None,
        has_password: false,
        tls: false,
    });
    assert_eq!(profile.validate(), Err(AppError::InvalidConnection));
}

#[test]
fn cluster_rejects_invalid_duplicate_oversized_and_drifting_seeds() {
    let mut profile = valid_profile();
    profile.host = "bad host".into();
    profile.port = 7000;
    profile.cluster = Some(ClusterConfig {
        nodes: vec![ConnectionEndpoint {
            host: "bad host".into(),
            port: 7000,
        }],
        read_from_replicas: false,
    });
    assert_eq!(profile.validate(), Err(AppError::InvalidConnection));

    profile.host = "127.0.0.1".into();
    profile.cluster.as_mut().unwrap().nodes = vec![
        ConnectionEndpoint {
            host: "127.0.0.1".into(),
            port: 7000,
        },
        ConnectionEndpoint {
            host: "127.0.0.1".into(),
            port: 7000,
        },
    ];
    assert_eq!(profile.validate(), Err(AppError::InvalidConnection));

    profile.cluster.as_mut().unwrap().nodes = (0..33)
        .map(|port| ConnectionEndpoint {
            host: "127.0.0.1".into(),
            port: 7000 + port,
        })
        .collect();
    assert_eq!(profile.validate(), Err(AppError::InvalidConnection));

    profile.cluster.as_mut().unwrap().nodes = vec![ConnectionEndpoint {
        host: "127.0.0.1".into(),
        port: 7001,
    }];
    assert_eq!(profile.validate(), Err(AppError::InvalidConnection));
}

#[test]
fn cluster_rejects_an_explicitly_empty_seed_list_with_invalid_connection_code() {
    let mut profile = valid_profile();
    profile.cluster = Some(ClusterConfig {
        nodes: vec![],
        read_from_replicas: false,
    });

    let error = profile.validate().unwrap_err();
    assert_eq!(error, AppError::InvalidConnection);
    assert_eq!(error.code(), "INVALID_CONNECTION");
}

#[test]
fn standalone_tls_and_ssh_are_valid_but_cluster_ssh_is_unsupported_at_target_selection() {
    let mut profile = valid_profile();
    profile.tls = true;
    profile.ssh = Some(
        serde_json::from_value(serde_json::json!({
            "host": "bastion.example", "port": 22, "username": "operator"
        }))
        .unwrap(),
    );
    assert_eq!(profile.validate(), Ok(()));

    profile.tls = false;
    profile.host = "127.0.0.1".into();
    profile.port = 7000;
    profile.cluster = Some(ClusterConfig {
        nodes: vec![ConnectionEndpoint {
            host: "127.0.0.1".into(),
            port: 7000,
        }],
        read_from_replicas: false,
    });
    assert_eq!(profile.validate(), Ok(()));
    assert_eq!(
        ConnectionTarget::try_from(&profile),
        Err(AppError::UnsupportedFeature)
    );
}

#[test]
fn array_and_vector_domain_contracts_reject_precision_and_payload_errors() {
    let input = ArrayRangeInput {
        connection_id: "local".into(),
        key: "events".into(),
        start: "18446744073709551615".into(),
        end: "18446744073709551616".into(),
    };
    assert_eq!(input.validate().unwrap_err(), AppError::InvalidInput);

    let query = VectorSimilarityQueryInput {
        connection_id: "local".into(),
        key: "embeddings".into(),
        by_element: Some("seed".into()),
        by_vector: Some(vec![1.0]),
        by_vector_base64: None,
        count: 10,
        with_attributes: true,
    };
    assert_eq!(query.validate().unwrap_err(), AppError::InvalidInput);
}

#[test]
fn normalizes_array_indexes_and_rejects_invalid_values() {
    assert_eq!(
        redix_lib::domain::normalize_array_index("00042").unwrap(),
        "42"
    );
    assert_eq!(
        redix_lib::domain::normalize_array_index("18446744073709551615").unwrap(),
        "18446744073709551615"
    );
    for index in ["", "-1", "+1", "18446744073709551616", "1.0"] {
        assert_eq!(
            redix_lib::domain::normalize_array_index(index).unwrap_err(),
            AppError::InvalidInput
        );
    }
}

#[test]
fn key_type_normalization_includes_module_types() {
    assert_eq!(
        redix_lib::domain::normalize_key_type("vectorset"),
        Some("vector-set")
    );
    assert_eq!(
        redix_lib::domain::normalize_key_type("vector-set"),
        Some("vector-set")
    );
    assert_eq!(
        redix_lib::domain::normalize_key_type("array"),
        Some("array")
    );
}

#[test]
fn validates_sparse_array_and_fp32_vector_payloads() {
    let sparse = CreateArrayInput {
        connection_id: "local".into(),
        key: "events".into(),
        mode: ArrayCreateMode::Sparse,
        start_index: None,
        values: vec![],
        elements: vec![
            ArrayElement {
                index: "01".into(),
                value: "first".into(),
            },
            ArrayElement {
                index: "1".into(),
                value: "duplicate".into(),
            },
        ],
        ttl_ms: None,
    };
    assert_eq!(sparse.validate().unwrap_err(), AppError::InvalidInput);

    let vector = VectorSetElementPayload {
        name: "seed".into(),
        vector_values: None,
        vector_fp32_base64: Some("AACAPw==".into()),
        attributes: Some(serde_json::json!({"kind": "seed"})),
    };
    assert_eq!(vector.validate(Some(1)), Ok(()));
    assert_eq!(
        vector.validate(Some(2)).unwrap_err(),
        AppError::InvalidInput
    );
}

#[test]
fn module_capabilities_require_new_command_sets() {
    let capabilities = ModuleCapabilities::from_modules_and_commands(
        vec![],
        ["ARLEN", "ARCOUNT", "ARSET"]
            .into_iter()
            .map(String::from)
            .collect(),
    );
    assert!(!capabilities.array_supported);
    assert!(!capabilities.vector_set_supported);
}

#[test]
fn serializes_profile_without_password_field() {
    let profile = valid_profile();
    let json = serde_json::to_value(&profile).unwrap();
    let object = json
        .as_object()
        .expect("profile must serialize as an object");

    assert!(!object.contains_key("password"));
    assert!(object.contains_key("has_password"));
}

#[test]
fn rejects_scan_counts_outside_one_through_five_hundred() {
    for count in [0, 501] {
        let input = ScanKeysInput {
            connection_id: "local".into(),
            cursor: 0.into(),
            pattern: "*".into(),
            count,
            key_type: None,
        };

        let error = input.validate().expect_err("invalid scan count must fail");
        assert_eq!(error.code(), "INVALID_CONNECTION");
    }
}

#[test]
fn scan_filter_rejects_unknown_key_type_and_accepts_supported_type() {
    let mut input = ScanKeysInput {
        connection_id: "local".into(),
        cursor: 0.into(),
        pattern: "*".into(),
        count: 100,
        key_type: Some("hash".into()),
    };
    assert_eq!(input.validate(), Ok(()));

    input.key_type = Some("vector".into());
    assert_eq!(input.validate().unwrap_err(), AppError::InvalidConnection);
}

#[test]
fn import_rejects_empty_entries_and_exported_key_rejects_empty_name() {
    assert_eq!(
        ImportKeysInput {
            connection_id: "local".into(),
            entries: vec![],
        }
        .validate()
        .unwrap_err(),
        AppError::InvalidConnection
    );

    let empty_name = ExportedKey {
        key: String::new(),
        ttl_ms: -1,
        value: RedisValue::String {
            value: "value".into(),
        },
    };
    assert_eq!(
        empty_name.validate().unwrap_err(),
        AppError::InvalidConnection
    );
}

#[test]
fn serializes_errors_with_stable_code_and_safe_message() {
    let json = serde_json::to_value(AppError::AuthenticationFailed).unwrap();

    assert_eq!(json["code"], "AUTHENTICATION_FAILED");
    assert_eq!(json["message"], "Redis 身份验证失败");
    assert!(!json.to_string().contains("password"));
}

#[test]
fn validates_browser_extension_inputs_without_accepting_empty_keys() {
    let value = RedisValue::Stream {
        entries: vec![StreamEntry {
            id: "1-0".into(),
            fields: vec![StreamField {
                field: "event".into(),
                value: "created".into(),
            }],
        }],
    };
    let valid = CreateKeyInput {
        connection_id: "local".into(),
        key: "events".into(),
        value,
        ttl_ms: Some(5_000),
    };
    assert_eq!(valid.validate(), Ok(()));

    let invalid_create = CreateKeyInput {
        key: " ".into(),
        ttl_ms: Some(-1),
        ..valid.clone()
    };
    assert_eq!(
        invalid_create.validate().unwrap_err().code(),
        "INVALID_CONNECTION"
    );

    let invalid_rename = RenameKeyInput {
        connection_id: "local".into(),
        key: "events".into(),
        new_key: " ".into(),
    };
    assert_eq!(
        invalid_rename.validate().unwrap_err().code(),
        "INVALID_CONNECTION"
    );

    let invalid_delete = DeleteKeysInput {
        connection_id: "local".into(),
        keys: vec!["events".into(), "".into()],
    };
    assert_eq!(
        invalid_delete.validate().unwrap_err().code(),
        "INVALID_CONNECTION"
    );

    let invalid_info = KeyInfoInput {
        connection_id: " ".into(),
        key: "events".into(),
    };
    assert_eq!(
        invalid_info.validate().unwrap_err().code(),
        "INVALID_CONNECTION"
    );
}

#[test]
fn rejects_empty_streams_and_stream_entries_without_leaking_values() {
    let empty_stream = CreateKeyInput {
        connection_id: "local".into(),
        key: "events".into(),
        value: RedisValue::Stream { entries: vec![] },
        ttl_ms: None,
    };
    let error = empty_stream
        .validate()
        .expect_err("empty stream must not create a key");

    assert_eq!(error.code(), "COMMAND_FAILED");
    assert_eq!(error.to_string(), "Redis 命令执行失败");
}

#[test]
fn accepts_json_root_documents_as_browser_values() {
    let input = CreateKeyInput {
        connection_id: "local".into(),
        key: "profile:1".into(),
        value: RedisValue::Json {
            value: serde_json::json!({"name": "Alice", "active": true}),
        },
        ttl_ms: None,
    };

    assert_eq!(input.validate(), Ok(()));
}

#[test]
fn json_path_inputs_reject_empty_or_unsafe_paths() {
    let input = GetJsonPathInput {
        connection_id: "local".into(),
        key: "doc".into(),
        path: "$.items[*]".into(),
    };

    assert_eq!(input.validate().unwrap_err(), AppError::InvalidInput);
    assert_eq!(validate_json_path("", false), Err(AppError::InvalidInput));
    assert_eq!(validate_json_path("$.user.name", false), Ok(()));
    assert_eq!(validate_json_path(".user.name", false), Ok(()));
    assert_eq!(validate_json_path("$[0]", false), Ok(()));
    assert_eq!(
        validate_json_path("$.items[0]['display.name']", false),
        Ok(())
    );
    assert_eq!(
        validate_json_path("$.items[0]['quote\\'and\\\\slash']", false),
        Ok(())
    );
    assert_eq!(
        validate_json_path("$[0:2]", false),
        Err(AppError::InvalidInput)
    );
    assert_eq!(
        validate_json_path("$['a','b']", false),
        Err(AppError::InvalidInput)
    );
    assert_eq!(
        validate_json_path("$.items[*]", false),
        Err(AppError::InvalidInput)
    );
    assert_eq!(
        validate_json_path("$.items[?(@.active)]", false),
        Err(AppError::InvalidInput)
    );
    assert_eq!(
        validate_json_path("$..items", false),
        Err(AppError::InvalidInput)
    );
    assert_eq!(
        validate_json_path("$.items.length()", false),
        Err(AppError::InvalidInput)
    );
    assert_eq!(
        validate_json_path("$.items[-1]", false),
        Err(AppError::InvalidInput)
    );
}

#[test]
fn json_path_supports_legacy_and_modern_root_forms() {
    assert_eq!(normalize_json_path("$", true).unwrap(), ".");
    assert_eq!(normalize_json_path("$.user", true).unwrap(), ".user");
    assert_eq!(normalize_json_path("$.user", false).unwrap(), "$.user");
}

#[test]
fn json_payload_and_array_append_limits_are_enforced() {
    let oversized = serde_json::Value::String("x".repeat(5 * 1024 * 1024));
    let input = SetJsonPathInput {
        connection_id: "local".into(),
        key: "doc".into(),
        path: "$".into(),
        value: oversized,
    };

    assert_eq!(input.validate().unwrap_err(), AppError::InvalidInput);
    assert_eq!(validate_json_array_append(&[]), Err(AppError::InvalidInput));
    assert_eq!(
        validate_json_array_append(&vec![serde_json::Value::Null; 501]),
        Err(AppError::InvalidInput)
    );
}

#[test]
fn json_payload_and_response_limits_accept_exact_boundary_and_reject_overflow() {
    let exact_limit = SetJsonPathInput {
        connection_id: "local".into(),
        key: "doc".into(),
        path: "$".into(),
        value: serde_json::Value::String("x".repeat((5 * 1024 * 1024) - 2)),
    };
    assert_eq!(exact_limit.validate(), Ok(()));

    let over_limit = SetJsonPathInput {
        value: serde_json::Value::String("x".repeat((5 * 1024 * 1024) - 1)),
        ..exact_limit
    };
    assert_eq!(over_limit.validate().unwrap_err(), AppError::InvalidInput);
}

#[test]
fn json_path_contract_enforces_trimmed_identifiers_and_utf8_byte_limit() {
    let input = GetJsonPathInput {
        connection_id: "  ".into(),
        key: "doc".into(),
        path: "$".into(),
    };
    assert_eq!(input.validate().unwrap_err(), AppError::InvalidInput);

    let input = GetJsonPathInput {
        connection_id: "local".into(),
        key: "  ".into(),
        path: "$".into(),
    };
    assert_eq!(input.validate().unwrap_err(), AppError::InvalidInput);

    let max_path = format!("$.{}", "你".repeat(170));
    assert_eq!(max_path.len(), 512);
    assert_eq!(validate_json_path(&max_path, false), Ok(()));

    let oversized_path = format!("$.{}", "你".repeat(171));
    assert!(oversized_path.len() > 512);
    assert_eq!(
        validate_json_path(&oversized_path, false),
        Err(AppError::InvalidInput)
    );
}

#[test]
fn append_json_array_input_accepts_boundary_and_rejects_limit_overflow() {
    let ok = AppendJsonArrayInput {
        connection_id: "local".into(),
        key: "doc".into(),
        path: ".items".into(),
        values: vec![serde_json::Value::Null; 500],
    };
    assert_eq!(ok.validate(), Ok(()));

    let too_many = AppendJsonArrayInput {
        values: vec![serde_json::Value::Null; 501],
        ..ok
    };
    assert_eq!(too_many.validate().unwrap_err(), AppError::InvalidInput);
}

#[test]
fn append_json_array_enforces_aggregate_payload_boundary() {
    let exact_limit = vec![
        serde_json::Value::String("x".repeat((5 * 1024 * 1024) - 4)),
        serde_json::Value::String(String::new()),
    ];
    assert_eq!(validate_json_array_append(&exact_limit), Ok(()));

    let over_limit = vec![
        serde_json::Value::String("x".repeat((5 * 1024 * 1024) - 3)),
        serde_json::Value::String(String::new()),
    ];
    assert_eq!(
        validate_json_array_append(&over_limit),
        Err(AppError::InvalidInput)
    );
}

#[test]
fn module_capabilities_detects_json_modules_case_insensitively() {
    let capabilities = ModuleCapabilities::from_modules(vec![
        ModuleSummary {
            name: "bf".into(),
            version: Some("1.0.0".into()),
        },
        ModuleSummary {
            name: "ReJSON".into(),
            version: Some("".into()),
        },
        ModuleSummary {
            name: "redisjson".into(),
            version: Some("2.6.8".into()),
        },
        ModuleSummary {
            name: "RedisJSON".into(),
            version: Some("2.8.0".into()),
        },
    ]);

    assert!(capabilities.json_supported);
    assert_eq!(capabilities.json_version.as_deref(), Some("2.6.8"));
}

#[test]
fn parses_info_sections_and_optional_metrics() {
    let sections = parse_info_sections(
        "# Server\nredis_version:7.2.5\nuptime_in_seconds:42\n\n# Clients\nconnected_clients:3\n# Keyspace\ndb0:keys=8,expires=2,avg_ttl=1200\n",
    );
    assert_eq!(sections["Server"]["redis_version"], "7.2.5");
    assert_eq!(sections["Clients"]["connected_clients"], "3");
    let db = parse_keyspace_line("db0", "keys=8,expires=2,avg_ttl=1200").unwrap();
    assert_eq!(db.database, 0);
    assert_eq!(db.key_count, Some(8));
    assert_eq!(db.expires, Some(2));
    assert_eq!(db.avg_ttl_ms, Some(1200));
}

#[test]
fn rejects_invalid_database_selection_and_keyspace_lines() {
    assert_eq!(
        SelectDatabaseInput {
            connection_id: "local".into(),
            database: 16,
        }
        .validate()
        .unwrap_err(),
        AppError::InvalidConnection
    );
    assert_eq!(
        parse_keyspace_line("dbx", "keys=1,expires=0").unwrap_err(),
        AppError::PersistenceFailed
    );
}

#[test]
fn query_library_rejects_sensitive_commands_and_accepts_normal_commands() {
    assert!(QueryLibraryItemInput {
        id: None,
        name: "读取用户".into(),
        command: "GET user:1".into(),
        tags: vec!["用户".into()],
    }
    .validate()
    .is_ok());
    assert_eq!(
        QueryLibraryItemInput {
            id: None,
            name: "认证".into(),
            command: "AUTH secret".into(),
            tags: vec![],
        }
        .validate()
        .unwrap_err(),
        AppError::InvalidConnection
    );
}

#[test]
fn settings_validate_fixed_enum_and_range() {
    assert!(AppSettings {
        version: 1,
        theme: "dark".into(),
        result_format: "json".into(),
        scan_count: 200,
        continue_on_error: true,
    }
    .validate()
    .is_ok());
    assert_eq!(
        AppSettings {
            scan_count: 1,
            ..AppSettings::default()
        }
        .validate()
        .unwrap_err(),
        AppError::InvalidConnection
    );
}

#[test]
fn rejects_invalid_observability_inputs_without_leaking_values() {
    let error = GetSlowLogsInput {
        connection_id: "".into(),
        count: 1001,
    }
    .validate()
    .unwrap_err();
    assert_eq!(error.code(), "INVALID_INPUT");
    assert_eq!(error.to_string(), "输入参数无效");

    let error = StartPubSubInput {
        connection_id: "local".into(),
        session_id: "session".into(),
        topics: vec![PubSubTopic {
            name: "   ".into(),
            pattern: false,
        }],
    }
    .validate()
    .unwrap_err();
    assert_eq!(error.code(), "INVALID_INPUT");
}

#[test]
fn normalizes_pubsub_topics_and_rejects_duplicates() {
    let input = StartPubSubInput {
        connection_id: "local".into(),
        session_id: "session".into(),
        topics: vec![
            PubSubTopic {
                name: " news.* ".into(),
                pattern: true,
            },
            PubSubTopic {
                name: "news.*".into(),
                pattern: true,
            },
        ],
    };
    assert_eq!(input.validate().unwrap_err().code(), "INVALID_INPUT");
}

#[test]
fn validates_profiler_session_inputs() {
    assert_eq!(
        StartProfilerInput {
            connection_id: "".into(),
            session_id: "session".into(),
        }
        .validate()
        .unwrap_err(),
        AppError::InvalidInput
    );
    assert_eq!(
        StopProfilerInput {
            connection_id: "local".into(),
            session_id: "".into(),
        }
        .validate()
        .unwrap_err(),
        AppError::InvalidInput
    );
}

#[test]
fn validates_stream_consumer_group_inputs() {
    assert_eq!(
        GetStreamConsumerGroupsInput {
            connection_id: "".into(),
            key: "events".into(),
        }
        .validate()
        .unwrap_err(),
        AppError::InvalidConnection
    );
    assert_eq!(
        GetStreamPendingEntriesInput {
            connection_id: "local".into(),
            key: "events".into(),
            group: "workers".into(),
            count: 501,
            consumer: None,
        }
        .validate()
        .unwrap_err(),
        AppError::InvalidConnection
    );
    assert_eq!(
        AcknowledgeStreamPendingEntriesInput {
            connection_id: "local".into(),
            key: "events".into(),
            group: "workers".into(),
            entries: vec![],
        }
        .validate()
        .unwrap_err(),
        AppError::InvalidConnection
    );
}

#[test]
fn search_create_input_rejects_empty_index_and_duplicate_fields() {
    let valid = CreateSearchIndexInput {
        connection_id: "local".into(),
        index: "idx:users".into(),
        key_type: SearchKeyType::Hash,
        prefixes: vec!["user:".into()],
        fields: vec![
            SearchIndexFieldInput {
                name: "name".into(),
                field_type: SearchFieldType::Text,
            },
            SearchIndexFieldInput {
                name: "name".into(),
                field_type: SearchFieldType::Tag,
            },
        ],
    };

    assert_eq!(
        CreateSearchIndexInput {
            index: String::new(),
            ..valid.clone()
        }
        .validate()
        .unwrap_err(),
        AppError::InvalidInput
    );
    assert_eq!(valid.validate().unwrap_err(), AppError::InvalidInput);
    assert_eq!(
        CreateSearchIndexInput {
            connection_id: " ".into(),
            ..valid
        }
        .validate()
        .unwrap_err(),
        AppError::InvalidConnection
    );
}

#[test]
fn search_create_input_enforces_field_and_prefix_limits() {
    let field = |index: usize| SearchIndexFieldInput {
        name: format!("field:{index}"),
        field_type: SearchFieldType::Text,
    };
    let base = CreateSearchIndexInput {
        connection_id: "local".into(),
        index: "idx:users".into(),
        key_type: SearchKeyType::Json,
        prefixes: vec!["user:".into()],
        fields: vec![field(0)],
    };

    assert_eq!(
        CreateSearchIndexInput {
            fields: (0..65).map(field).collect(),
            ..base.clone()
        }
        .validate()
        .unwrap_err(),
        AppError::InvalidInput
    );
    assert_eq!(
        CreateSearchIndexInput {
            prefixes: (0..65).map(|index| format!("user:{index}:")).collect(),
            ..base.clone()
        }
        .validate()
        .unwrap_err(),
        AppError::InvalidInput
    );
    assert_eq!(
        CreateSearchIndexInput {
            fields: vec![SearchIndexFieldInput {
                name: "x".repeat(257),
                field_type: SearchFieldType::Text,
            }],
            ..base.clone()
        }
        .validate()
        .unwrap_err(),
        AppError::InvalidInput
    );
    assert_eq!(
        CreateSearchIndexInput {
            prefixes: vec!["x".repeat(257)],
            ..base.clone()
        }
        .validate()
        .unwrap_err(),
        AppError::InvalidInput
    );

    let boundary = CreateSearchIndexInput {
        prefixes: (0..64).map(|index| format!("user:{index}:")).collect(),
        ..base
    };
    assert_eq!(boundary.validate(), Ok(()));
}

#[test]
fn search_query_input_enforces_query_offset_and_limit() {
    let valid = SearchQueryInput {
        connection_id: "local".into(),
        index: "idx:users".into(),
        query: "*".into(),
        offset: 100_000,
        limit: 200,
        include_content: false,
    };

    for invalid in [
        SearchQueryInput {
            query: " ".into(),
            ..valid.clone()
        },
        SearchQueryInput {
            query: "x".repeat(4097),
            ..valid.clone()
        },
        SearchQueryInput {
            offset: 100_001,
            ..valid.clone()
        },
        SearchQueryInput {
            limit: 0,
            ..valid.clone()
        },
        SearchQueryInput {
            limit: 201,
            ..valid.clone()
        },
    ] {
        assert_eq!(invalid.validate().unwrap_err(), AppError::InvalidInput);
    }
    assert_eq!(valid.validate(), Ok(()));
}

#[test]
fn search_capabilities_recognize_search_module_names_and_versions() {
    for name in ["Search", "RediSearch", "redisearch"] {
        let capabilities = ModuleCapabilities::from_modules(vec![ModuleSummary {
            name: name.into(),
            version: Some("2.6.11".into()),
        }]);
        assert!(capabilities.search_supported, "module name {name}");
        assert_eq!(capabilities.search_version.as_deref(), Some("2.6.11"));
        assert!(capabilities.search_compatible());
    }

    let no_search = ModuleCapabilities::from_modules(vec![]);
    assert!(!no_search.search_supported);
    assert!(!no_search.search_compatible());
    assert!(!search_version_supported(None));
    assert!(!search_version_supported(Some("1.6.0")));
    assert!(search_version_supported(Some("2.0.0")));
    assert!(search_version_supported(Some("2.10.0")));
}

#[test]
fn unsupported_feature_serializes_to_stable_ipc_error() {
    let json = serde_json::to_value(AppError::UnsupportedFeature).unwrap();
    assert_eq!(
        json,
        serde_json::json!({
            "code": "UNSUPPORTED_FEATURE",
            "message": "当前 Redis 功能不可用"
        })
    );
}
