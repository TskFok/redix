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
