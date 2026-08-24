use std::{collections::HashMap, sync::Arc};

use redix_lib::domain::{
    parse_command_stats, AnalysisAccumulator, AnalysisKeyMetadata, AnalyzeDatabaseInput,
    InstanceDetails, ModuleSummary,
};
use redix_lib::{
    error::AppError,
    redis::{RedisOperations, RedisService},
};

mod fixtures {
    use redix_lib::{
        domain::ConnectionProfile,
        error::AppError,
        persistence::{ConnectionSecrets, ProfileRepository, SecretStore},
    };

    #[derive(Default)]
    pub struct TestProfiles {
        pub profiles: Vec<ConnectionProfile>,
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
    pub struct TestSecrets;

    impl SecretStore for TestSecrets {
        fn read(&self, _connection_id: &str) -> Result<Option<ConnectionSecrets>, AppError> {
            Ok(None)
        }

        fn write(
            &self,
            _connection_id: &str,
            _secrets: &ConnectionSecrets,
        ) -> Result<(), AppError> {
            Ok(())
        }

        fn delete(&self, _connection_id: &str) -> Result<(), AppError> {
            Ok(())
        }
    }
}

use fixtures::{TestProfiles, TestSecrets};

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
        service
            .analyze_database(AnalyzeDatabaseInput {
                connection_id: "missing".into(),
                pattern: "*".into(),
                delimiter: ":".into(),
                max_keys: 1000,
            })
            .await
            .unwrap_err(),
        AppError::ConnectionFailed
    );
}

#[test]
fn validates_analysis_input_limits() {
    let mut input = AnalyzeDatabaseInput {
        connection_id: "local".into(),
        pattern: "*".into(),
        delimiter: ":".into(),
        max_keys: 100_000,
    };
    assert_eq!(input.validate(), Ok(()));

    input.max_keys = 1_000_000;
    assert_eq!(input.validate(), Ok(()));

    input.max_keys = 1_000_001;
    assert_eq!(input.validate().unwrap_err().code(), "INVALID_INPUT");

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
    let report = accumulator.finish(3, 2, false);
    assert_eq!(report.database, 0);
    assert_eq!(report.progress.scanned, 3);
    assert_eq!(report.progress.processed, 2);
    assert_eq!(report.total_keys.total, 2);
    assert_eq!(report.total_keys.observed, 2);
    assert_eq!(report.total_memory.total, 384);
    assert_eq!(report.total_memory.observed, 2);
    assert_eq!(report.top_namespaces_by_keys[0].namespace, "user");
    assert_eq!(report.top_keys_by_memory[0].key, "user:2");
    assert!(report
        .expiration_groups
        .iter()
        .any(|group| group.label == "No Expiry"));
    assert!(!report.progress.truncated);
}

#[test]
fn parses_commandstats_and_optional_instance_metrics_without_raw_text() {
    let mut sections = HashMap::new();
    sections.insert(
        "Server".into(),
        HashMap::from([("redis_version".into(), "7.2.5".into())]),
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

    let details = InstanceDetails::from_info_and_modules(
        &sections,
        vec![ModuleSummary {
            name: "ReJSON".into(),
            version: Some("2.8.10".into()),
        }],
    )
    .unwrap();
    assert_eq!(details.overview.server_version.as_deref(), Some("7.2.5"));
    assert_eq!(details.clients.blocked_clients, Some(1));
    assert_eq!(parse_command_stats(&sections)[0].command, "GET");
}

#[test]
fn maps_standard_connected_slaves_to_connected_replicas() {
    let sections = HashMap::from([(
        "Replication".into(),
        HashMap::from([("connected_slaves".into(), "2".into())]),
    )]);

    let details = InstanceDetails::from_info_and_modules(&sections, vec![]).unwrap();

    assert_eq!(details.replication.connected_replicas, Some(2));
}

#[test]
fn malformed_optional_info_fields_degrade_independently() {
    let sections = HashMap::from([
        (
            "Server".into(),
            HashMap::from([
                ("redis_version".into(), "7.2.5".into()),
                ("uptime_in_seconds".into(), "not-a-number".into()),
            ]),
        ),
        (
            "Clients".into(),
            HashMap::from([
                ("connected_clients".into(), "invalid".into()),
                ("blocked_clients".into(), "1".into()),
            ]),
        ),
        (
            "Memory".into(),
            HashMap::from([
                ("used_memory".into(), "invalid".into()),
                ("used_memory_peak".into(), "2048".into()),
                ("mem_fragmentation_ratio".into(), "NaN".into()),
            ]),
        ),
        (
            "Stats".into(),
            HashMap::from([
                ("total_commands_processed".into(), "invalid".into()),
                ("keyspace_hits".into(), "invalid".into()),
                ("keyspace_misses".into(), "2".into()),
                ("instantaneous_ops_per_sec".into(), "invalid".into()),
                ("expired_keys".into(), "4".into()),
            ]),
        ),
        (
            "Persistence".into(),
            HashMap::from([
                ("loading".into(), "invalid".into()),
                ("rdb_last_save_time".into(), "invalid".into()),
                ("aof_enabled".into(), "1".into()),
            ]),
        ),
        (
            "Replication".into(),
            HashMap::from([
                ("role".into(), "master".into()),
                ("connected_slaves".into(), "3".into()),
                ("master_repl_offset".into(), "invalid".into()),
            ]),
        ),
    ]);

    let details = InstanceDetails::from_info_and_modules(&sections, vec![]).unwrap();

    assert_eq!(details.overview.server_version.as_deref(), Some("7.2.5"));
    assert_eq!(details.overview.uptime_seconds, None);
    assert_eq!(details.overview.connected_clients, None);
    assert_eq!(details.overview.used_memory_bytes, None);
    assert_eq!(details.overview.total_commands_processed, None);
    assert_eq!(details.clients.blocked_clients, Some(1));
    assert_eq!(details.memory.used_memory_bytes, None);
    assert_eq!(details.memory.used_memory_peak_bytes, Some(2048));
    assert_eq!(details.memory.mem_fragmentation_ratio, None);
    assert_eq!(details.stats.instantaneous_ops_per_sec, None);
    assert_eq!(details.stats.expired_keys, Some(4));
    assert_eq!(details.stats.hit_rate, None);
    assert_eq!(details.persistence.loading, None);
    assert_eq!(details.persistence.rdb_last_save_time, None);
    assert_eq!(details.persistence.aof_enabled, Some(true));
    assert_eq!(details.replication.role.as_deref(), Some("master"));
    assert_eq!(details.replication.connected_replicas, Some(3));
    assert_eq!(details.replication.master_repl_offset, None);
}

#[test]
fn degrades_hit_rate_to_none_when_info_counters_overflow() {
    let sections = HashMap::from([(
        "Stats".into(),
        HashMap::from([
            ("keyspace_hits".into(), u64::MAX.to_string()),
            ("keyspace_misses".into(), "1".into()),
        ]),
    )]);

    let details = InstanceDetails::from_info_and_modules(&sections, vec![]).unwrap();

    assert_eq!(details.stats.hit_rate, None);
}
