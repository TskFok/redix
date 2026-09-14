use base64::{engine::general_purpose::STANDARD, Engine};
use redix_lib::{
    domain::{value_codec::*, ConnectionProfile},
    error::AppError,
    persistence::{ConnectionSecrets, ProfileRepository, SecretStore},
    redis::{RedisOperations, RedisService},
};
use std::{
    net::TcpListener,
    process::{Child, Command, Stdio},
    sync::Arc,
    time::{Duration, Instant},
};

struct IsolatedRedis {
    process: Child,
    _directory: tempfile::TempDir,
}
impl Drop for IsolatedRedis {
    fn drop(&mut self) {
        let _ = self.process.kill();
        let _ = self.process.wait();
    }
}
impl IsolatedRedis {
    fn start() -> (Self, u16) {
        let reservation = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = reservation.local_addr().unwrap().port();
        drop(reservation);
        let directory = tempfile::tempdir().unwrap();
        let process = Command::new("redis-server")
            .args([
                "--bind",
                "127.0.0.1",
                "--port",
                &port.to_string(),
                "--save",
                "",
                "--appendonly",
                "no",
                "--dir",
            ])
            .arg(directory.path())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("测试需要 redis-server");
        let mut server = Self {
            process,
            _directory: directory,
        };
        let client = redis::Client::open(format!("redis://127.0.0.1:{port}/")).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            assert!(
                server.process.try_wait().unwrap().is_none(),
                "临时 Redis 提前退出"
            );
            if let Ok(mut connection) = client.get_connection() {
                if let Ok(info) = redis::cmd("INFO")
                    .arg("server")
                    .query::<String>(&mut connection)
                {
                    if info
                        .lines()
                        .any(|line| line == format!("process_id:{}", server.process.id()))
                    {
                        break;
                    }
                }
            }
            assert!(
                Instant::now() < deadline,
                "临时 Redis 未就绪或端口被其他进程占用"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
        (server, port)
    }
}
struct Profiles(ConnectionProfile);
impl ProfileRepository for Profiles {
    fn load(&self) -> Result<Vec<ConnectionProfile>, AppError> {
        Ok(vec![self.0.clone()])
    }
    fn save(&self, _: &[ConnectionProfile]) -> Result<(), AppError> {
        Ok(())
    }
}
struct Secrets(Option<String>);
impl SecretStore for Secrets {
    fn read(&self, _: &str) -> Result<Option<ConnectionSecrets>, AppError> {
        Ok(self.0.clone().map(|password| ConnectionSecrets {
            password: Some(password),
            ..ConnectionSecrets::default()
        }))
    }
    fn write(&self, _: &str, _: &ConnectionSecrets) -> Result<(), AppError> {
        Ok(())
    }
    fn delete(&self, _: &str) -> Result<(), AppError> {
        Ok(())
    }
}
fn target(key: &str) -> GetStringValueInput {
    GetStringValueInput {
        connection_id: "codec-test".into(),
        key: key.into(),
    }
}
fn mutation(key: &str, bytes: &[u8]) -> SetStringValueInput {
    SetStringValueInput {
        connection_id: "codec-test".into(),
        key: key.into(),
        base64: STANDARD.encode(bytes),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "启动并销毁临时 Redis；使用 --test string_value_integration -- --ignored"]
async fn binary_string_reads_are_bounded_and_saves_preserve_ttl_without_recreating_keys() {
    let (_server, port) = IsolatedRedis::start();
    let profile = ConnectionProfile {
        id: "codec-test".into(),
        name: "Codec test".into(),
        host: "127.0.0.1".into(),
        port,
        username: None,
        database: 0,
        has_password: false,
        tls: false,
        verify_server_cert: true,
        ssh: None,
        sentinel: None,
        cluster: None,
        ca_certificate_name: None,
        client_certificate_name: None,
        has_ca_certificate: false,
        has_client_certificate: false,
    };
    let service = RedisService::new(Arc::new(Profiles(profile.clone())), Arc::new(Secrets(None)));
    service.open_connection("codec-test").await.unwrap();
    let client = redis::Client::open(format!("redis://127.0.0.1:{port}/")).unwrap();
    let mut connection = client.get_multiplexed_async_connection().await.unwrap();
    let bytes: Vec<u8> = (0..=255).collect();
    redis::cmd("SET")
        .arg("binary")
        .arg(&bytes)
        .arg("PX")
        .arg(60_000)
        .query_async::<()>(&mut connection)
        .await
        .unwrap();
    let page = service.get_string_value(target("binary")).await.unwrap();
    assert_eq!(STANDARD.decode(page.base64).unwrap(), bytes);
    assert_eq!(page.total_bytes, 256);
    assert!(!page.truncated);
    let saved = service
        .set_string_value(mutation("binary", &[0, 255, 128]))
        .await
        .unwrap();
    assert!(saved.ttl_ms > 0 && saved.ttl_ms <= page.ttl_ms);
    assert_eq!(saved.byte_length, 3);
    assert_eq!(
        redis::cmd("GET")
            .arg("binary")
            .query_async::<Vec<u8>>(&mut connection)
            .await
            .unwrap(),
        [0, 255, 128]
    );
    let preview = service
        .get_browser_key("codec-test", "binary")
        .await
        .unwrap();
    assert_eq!(preview.key_type, "string");
    redis::cmd("SET")
        .arg("permanent")
        .arg("old")
        .query_async::<()>(&mut connection)
        .await
        .unwrap();
    assert_eq!(
        service
            .set_string_value(mutation("permanent", &[]))
            .await
            .unwrap()
            .ttl_ms,
        -1
    );
    assert_eq!(
        service
            .get_string_value(target("permanent"))
            .await
            .unwrap()
            .total_bytes,
        0
    );

    assert_eq!(
        service
            .set_string_value(mutation("missing", b"new"))
            .await
            .unwrap_err(),
        AppError::KeyNotFound
    );
    assert_eq!(
        redis::cmd("EXISTS")
            .arg("missing")
            .query_async::<i64>(&mut connection)
            .await
            .unwrap(),
        0
    );
    redis::cmd("HSET")
        .arg("hash")
        .arg("field")
        .arg("value")
        .query_async::<()>(&mut connection)
        .await
        .unwrap();
    assert_eq!(
        service
            .set_string_value(mutation("hash", b"new"))
            .await
            .unwrap_err(),
        AppError::UnsupportedDataType
    );
    assert_eq!(
        service.get_string_value(target("hash")).await.unwrap_err(),
        AppError::UnsupportedDataType
    );
    assert_eq!(
        redis::cmd("HGET")
            .arg("hash")
            .arg("field")
            .query_async::<String>(&mut connection)
            .await
            .unwrap(),
        "value"
    );

    let oversized = vec![0xff; MAX_STRING_BYTES + 1];
    redis::cmd("SET")
        .arg("large")
        .arg(&oversized)
        .query_async::<()>(&mut connection)
        .await
        .unwrap();
    let large = service.get_string_value(target("large")).await.unwrap();
    assert!(large.truncated);
    assert_eq!(large.total_bytes, oversized.len() as u64);
    assert_eq!(
        STANDARD.decode(large.base64).unwrap().len(),
        MAX_STRING_BYTES
    );
    assert_eq!(
        service
            .set_string_value(mutation("binary", &oversized))
            .await
            .unwrap_err(),
        AppError::InvalidInput
    );
    assert_eq!(
        service
            .get_string_value(target("missing"))
            .await
            .unwrap_err(),
        AppError::KeyNotFound
    );

    // A read-only user intentionally has no scripting or transaction permissions.
    redis::cmd("ACL")
        .arg("SETUSER")
        .arg("codec-reader")
        .arg("on")
        .arg(">isolated-test-password")
        .arg("~*")
        .arg("+type")
        .arg("+strlen")
        .arg("+pttl")
        .arg("+getrange")
        .arg("+ping")
        .arg("+info")
        .arg("+client")
        .arg("+select")
        .query_async::<()>(&mut connection)
        .await
        .unwrap();
    let readonly = RedisService::new(
        Arc::new(Profiles(ConnectionProfile {
            username: Some("codec-reader".into()),
            has_password: true,
            ..profile
        })),
        Arc::new(Secrets(Some("isolated-test-password".into()))),
    );
    readonly.open_connection("codec-test").await.unwrap();
    let readonly_page = readonly.get_string_value(target("binary")).await.unwrap();
    assert_eq!(
        STANDARD.decode(readonly_page.base64).unwrap(),
        [0, 255, 128]
    );
    assert!(readonly
        .set_string_value(mutation("binary", b"denied"))
        .await
        .is_err());
    readonly.close_connection("codec-test").await.unwrap();
    service.close_connection("codec-test").await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "启动并销毁临时 Redis；覆盖二进制键和整值导入导出"]
async fn binary_keys_and_whole_values_round_trip_without_display_identity_collisions() {
    use redix_lib::domain::{
        CreateKeyInput, DeleteKeysInput, ExportKeysInput, HashEntry, ImportKeysInput, KeyInfoInput,
        RedisBytes, RedisValue, RenameKeyInput, ScanAllKeysInput, SetKeyTtlInput, SortedSetEntry,
    };
    let (_server, port) = IsolatedRedis::start();
    let profile: ConnectionProfile = serde_json::from_value(serde_json::json!({
        "id":"codec-test", "name":"Binary keys", "host":"127.0.0.1", "port":port,
        "username":null, "database":0, "has_password":false, "tls":false,
        "verify_server_cert":true, "ssh":null, "sentinel":null, "cluster":null,
        "ca_certificate_name":null, "client_certificate_name":null,
        "has_ca_certificate":false, "has_client_certificate":false
    }))
    .unwrap();
    let service = RedisService::new(Arc::new(Profiles(profile)), Arc::new(Secrets(None)));
    service.open_connection("codec-test").await.unwrap();
    let all: RedisBytes = (0..=255).collect::<Vec<u8>>().into();
    let values = vec![
        RedisValue::String { value: all.clone() },
        RedisValue::Hash {
            fields: vec![
                HashEntry {
                    field: all.clone(),
                    value: all.clone(),
                },
                HashEntry {
                    field: "".into(),
                    value: "".into(),
                },
            ],
        },
        RedisValue::List {
            items: vec![all.clone(), "".into(), "\0".into()],
        },
        RedisValue::Set {
            members: vec![all.clone()],
        },
        RedisValue::SortedSet {
            members: vec![
                SortedSetEntry {
                    member: all.clone(),
                    score: 1.5,
                },
                SortedSetEntry {
                    member: "".into(),
                    score: 2.0,
                },
            ],
        },
    ];
    let keys: Vec<RedisBytes> = (0..values.len())
        .map(|i| vec![255, 0, i as u8].into())
        .collect();
    for (key, value) in keys.iter().zip(&values) {
        let created = service
            .create_key(CreateKeyInput {
                connection_id: "codec-test".into(),
                key: key.clone(),
                value: value.clone(),
                ttl_ms: Some(60_000),
            })
            .await
            .unwrap();
        assert_eq!(&created.key, key);
        // Redis hash order is unspecified, compare its entries independently below.
        if !matches!(value, RedisValue::Hash { .. }) {
            assert_eq!(&created.value, value);
        }
        let info = service
            .get_key_info(KeyInfoInput {
                connection_id: "codec-test".into(),
                key: key.clone(),
            })
            .await
            .unwrap();
        assert_eq!(&info.key, key);
        assert!(info.ttl_ms > 0);
    }
    // Empty keys, whitespace and text that resembles a binary display remain distinct.
    for key in [
        RedisBytes::from(""),
        RedisBytes::from(" "),
        RedisBytes::from("base64:/wAA"),
    ] {
        service
            .create_key(CreateKeyInput {
                connection_id: "codec-test".into(),
                key,
                value: RedisValue::String {
                    value: "untouched".into(),
                },
                ttl_ms: None,
            })
            .await
            .unwrap();
    }
    let scanned = service
        .scan_all_keys(ScanAllKeysInput {
            connection_id: "codec-test".into(),
            pattern: "*".into(),
            count: 100,
            key_type: None,
        })
        .await
        .unwrap();
    assert_eq!(scanned.len(), 8);
    for key in &keys {
        assert!(scanned.iter().any(|entry| &entry.key == key));
    }
    let raw = service
        .get_string_value(GetStringValueInput {
            connection_id: "codec-test".into(),
            key: keys[0].clone(),
        })
        .await
        .unwrap();
    assert_eq!(STANDARD.decode(raw.base64).unwrap(), all.as_bytes());
    service
        .set_string_value(SetStringValueInput {
            connection_id: "codec-test".into(),
            key: keys[0].clone(),
            base64: STANDARD.encode([254, 0, 255]),
        })
        .await
        .unwrap();
    assert_eq!(
        service.get_key("codec-test", &keys[0]).await.unwrap().value,
        RedisValue::String {
            value: vec![254, 0, 255].into()
        }
    );
    assert!(
        service
            .set_key_ttl(SetKeyTtlInput {
                connection_id: "codec-test".into(),
                key: keys[0].clone(),
                ttl_ms: 30_000
            })
            .await
            .unwrap()
            > 0
    );
    let exported = service
        .export_keys(ExportKeysInput {
            connection_id: "codec-test".into(),
            keys: keys.clone(),
        })
        .await
        .unwrap();
    let wire = serde_json::to_value(&exported).unwrap();
    assert!(wire[0]["key"].is_object());
    let mut imported: Vec<redix_lib::domain::ExportedKey> = serde_json::from_value(wire).unwrap();
    for entry in &mut imported {
        let mut bytes = b"copy:".to_vec();
        bytes.extend_from_slice(entry.key.as_bytes());
        entry.key = bytes.into();
    }
    assert_eq!(
        service
            .import_keys(ImportKeysInput {
                connection_id: "codec-test".into(),
                entries: imported.clone()
            })
            .await
            .unwrap(),
        5
    );
    assert_eq!(
        service
            .import_keys(ImportKeysInput {
                connection_id: "codec-test".into(),
                entries: imported.clone()
            })
            .await
            .unwrap(),
        0
    );
    for (source, copy) in exported.iter().zip(&imported) {
        let actual = service.get_key("codec-test", &copy.key).await.unwrap();
        assert!(actual.ttl_ms > 0 && actual.ttl_ms <= source.ttl_ms);
        match (&source.value, actual.value) {
            (RedisValue::Hash { fields: source }, RedisValue::Hash { fields: actual }) => {
                assert_eq!(source.len(), actual.len());
                assert!(source.iter().all(|entry| actual.contains(entry)));
            }
            (_, value) => assert_eq!(source.value, value),
        }
    }
    let renamed: RedisBytes = vec![255, 0, 99].into();
    assert_eq!(
        service
            .rename_browser_key(RenameKeyInput {
                connection_id: "codec-test".into(),
                key: keys[0].clone(),
                new_key: renamed.clone()
            })
            .await
            .unwrap()
            .key,
        renamed
    );
    service.delete_key("codec-test", &renamed).await.unwrap();
    assert_eq!(
        service
            .get_key("codec-test", "base64:/wAA")
            .await
            .unwrap()
            .value,
        RedisValue::String {
            value: "untouched".into()
        }
    );
    assert_eq!(
        service
            .delete_keys(DeleteKeysInput {
                connection_id: "codec-test".into(),
                keys: vec!["".into(), " ".into()]
            })
            .await
            .unwrap(),
        2
    );
    service.close_connection("codec-test").await.unwrap();
}
