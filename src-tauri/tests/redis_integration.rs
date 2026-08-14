use std::{
    collections::HashMap,
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};

use redix_lib::{
    domain::{ConnectionProfile, RedisValue, ScanKeysInput, SetKeyInput, SetKeyTtlInput},
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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "设置 REDIX_TEST_REDIS_URL 后运行本地 Redis 集成测试"]
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
    let info = service.open_connection("integration").await.unwrap();
    assert!(!info.server_version.is_empty());

    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let prefix = format!("redix:task4:{}:{suffix}", std::process::id());
    let string_key = format!("{prefix}:string");
    let hash_key = format!("{prefix}:hash");
    let list_key = format!("{prefix}:list");
    let set_key = format!("{prefix}:set");
    let zset_key = format!("{prefix}:zset");

    let string_value = service
        .set_key(SetKeyInput {
            connection_id: "integration".into(),
            key: string_key.clone(),
            value: RedisValue::String {
                value: "hello".into(),
            },
        })
        .await
        .unwrap();
    assert_eq!(
        string_value.value,
        RedisValue::String {
            value: "hello".into()
        }
    );

    for (key, value) in [
        (
            hash_key.clone(),
            RedisValue::Hash {
                fields: vec![redix_lib::domain::HashEntry {
                    field: "field".into(),
                    value: "value".into(),
                }],
            },
        ),
        (
            list_key.clone(),
            RedisValue::List {
                items: vec!["a".into(), "b".into()],
            },
        ),
        (
            set_key.clone(),
            RedisValue::Set {
                members: vec!["a".into(), "b".into()],
            },
        ),
        (
            zset_key.clone(),
            RedisValue::SortedSet {
                members: vec![redix_lib::domain::SortedSetEntry {
                    member: "member".into(),
                    score: 1.5,
                }],
            },
        ),
    ] {
        service
            .set_key(SetKeyInput {
                connection_id: "integration".into(),
                key,
                value,
            })
            .await
            .unwrap();
    }

    let mut cursor = 0;
    let mut scanned = Vec::new();
    loop {
        let page = service
            .scan_keys(ScanKeysInput {
                connection_id: "integration".into(),
                cursor,
                pattern: format!("{prefix}:*"),
                count: 100,
            })
            .await
            .unwrap();
        scanned.extend(page.keys.into_iter().map(|summary| summary.key));
        cursor = page.cursor;
        if cursor == 0 {
            break;
        }
    }
    assert!(scanned.contains(&string_key));
    assert!(scanned.contains(&zset_key));

    let ttl = service
        .set_key_ttl(SetKeyTtlInput {
            connection_id: "integration".into(),
            key: string_key.clone(),
            ttl_ms: 5_000,
        })
        .await
        .unwrap();
    assert!(ttl > 0);
    let command = service
        .execute_command("integration", "PING")
        .await
        .unwrap();
    assert_eq!(command.kind, "string");

    for key in [string_key, hash_key, list_key, set_key, zset_key] {
        service.delete_key("integration", &key).await.unwrap();
    }
    service.close_connection("integration").await.unwrap();
}
