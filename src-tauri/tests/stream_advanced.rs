use redix_lib::domain::stream_advanced::*;

fn query() -> GetStreamPendingPageInput {
    GetStreamPendingPageInput {
        connection_id: "local".into(),
        key: "events".into(),
        group: "workers".into(),
        consumer: None,
        start: "-".into(),
        end: "+".into(),
        cursor: None,
        count: 100,
    }
}

#[test]
fn pending_range_validates_precise_ids_capacity_and_cursor_scope() {
    assert!(query().validate().is_ok());
    for count in [0, 501, u32::MAX] {
        let mut input = query();
        input.count = count;
        assert!(input.validate().is_err());
    }
    let mut input = query();
    input.start = "9007199254740993-1".into();
    input.end = "9007199254740993-2".into();
    input.cursor = Some(input.start.clone());
    assert!(input.validate().is_ok());
    input.cursor = Some("9007199254740993-3".into());
    assert!(input.validate().is_err());
    input = query();
    input.start = "2-0".into();
    input.end = "1-0".into();
    assert!(input.validate().is_err());
    input = query();
    input.consumer = Some(" ".into());
    assert!(input.validate().is_err());
}

#[test]
fn setid_accepts_latest_incomplete_and_exact_ids_but_not_wildcards() {
    let mut input = UpdateStreamGroupIdInput {
        connection_id: "local".into(),
        key: "events".into(),
        group: "workers".into(),
        last_delivered_id: "$".into(),
    };
    for id in ["$", "0", "123", "18446744073709551615-18446744073709551615"] {
        input.last_delivered_id = id.into();
        assert!(input.validate().is_ok(), "{id}");
    }
    for id in ["*", "1-*", "-1", "1-0-2", "18446744073709551616", " "] {
        input.last_delivered_id = id.into();
        assert!(input.validate().is_err(), "{id}");
    }
}

#[test]
fn advanced_claim_rejects_empty_ids_and_unsafe_numeric_options() {
    let mut input = ClaimStreamPendingAdvancedInput {
        connection_id: "local".into(),
        key: "events".into(),
        group: "workers".into(),
        consumer: "new".into(),
        min_idle_ms: 0,
        entries: vec!["1-0".into()],
        idle_ms: Some(0),
        time_ms: None,
        retry_count: Some(0),
        force: true,
    };
    assert!(input.validate().is_ok());
    input.time_ms = Some(-1);
    assert!(input.validate().is_ok());
    input.idle_ms = Some(u64::MAX);
    assert!(input.validate().is_err());
    input.idle_ms = None;
    input.time_ms = Some(i64::MIN);
    assert!(input.validate().is_err());
    input.time_ms = None;
    input.retry_count = Some(u64::MAX);
    assert!(input.validate().is_err());
    input.retry_count = None;
    input.entries = vec![];
    assert!(input.validate().is_err());
    input.entries = vec!["1-0".into(); 501];
    assert!(input.validate().is_err());
}
