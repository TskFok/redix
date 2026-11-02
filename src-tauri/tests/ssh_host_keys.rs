use redix_lib::{error::AppError, persistence::SshHostKeyStore};
use ssh2::HostKeyType;

fn ed25519_key(byte: u8) -> Vec<u8> {
    let mut key = Vec::new();
    key.extend_from_slice(&11_u32.to_be_bytes());
    key.extend_from_slice(b"ssh-ed25519");
    key.extend_from_slice(&32_u32.to_be_bytes());
    key.extend_from_slice(&[byte; 32]);
    key
}

fn unknown_id(result: Result<(), AppError>) -> uuid::Uuid {
    match result {
        Err(AppError::SshHostKeyUnknown(id)) => id,
        other => panic!("expected an unknown host challenge, got {other:?}"),
    }
}

#[test]
fn unknown_host_requires_confirmation_before_persistent_trust() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("known_hosts");
    let store = SshHostKeyStore::new(path.clone());
    let key = ed25519_key(7);
    let id = unknown_id(store.verify("example.com", 22, &key, HostKeyType::Ed25519));
    let challenge = store.get_challenge(id).unwrap();
    assert_eq!(challenge.challenge_id, id.to_string());
    assert_eq!(challenge.host, "example.com");
    assert_eq!(challenge.port, 22);
    assert_eq!(challenge.algorithm, "ssh-ed25519");
    assert_eq!(
        challenge.fingerprint,
        "SHA256:gNSIRW+2Iyiuvsdp/bgjy38bvWHw6wQm3tuoXrl3WjQ"
    );
    assert!(
        !path.exists(),
        "a challenge must not implicitly trust the host"
    );

    store.trust(id).unwrap();
    let fresh_store = SshHostKeyStore::new(path);
    assert_eq!(
        fresh_store.verify("example.com", 22, &key, HostKeyType::Ed25519),
        Ok(())
    );
    assert_eq!(store.trust(id), Err(AppError::SshHostKeyChallengeExpired));
}

#[test]
fn confirmed_key_cannot_authorize_a_changed_key_or_a_different_endpoint() {
    let directory = tempfile::tempdir().unwrap();
    let store = SshHostKeyStore::new(directory.path().join("known_hosts"));
    let key = ed25519_key(7);
    let id = unknown_id(store.verify("example.com", 22, &key, HostKeyType::Ed25519));
    store.trust(id).unwrap();
    assert_eq!(
        store.verify("example.com", 22, &ed25519_key(8), HostKeyType::Ed25519),
        Err(AppError::SshHostKeyChanged)
    );
    unknown_id(store.verify("example.com", 2222, &key, HostKeyType::Ed25519));
    unknown_id(store.verify("other.example", 22, &key, HostKeyType::Ed25519));
}

#[test]
fn concurrent_challenges_cannot_overwrite_a_different_confirmed_key() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("known_hosts");
    let first = SshHostKeyStore::new(path.clone());
    let second = SshHostKeyStore::new(path);
    let first_id =
        unknown_id(first.verify("example.com", 22, &ed25519_key(7), HostKeyType::Ed25519));
    let second_id =
        unknown_id(second.verify("example.com", 22, &ed25519_key(8), HostKeyType::Ed25519));
    first.trust(first_id).unwrap();
    assert_eq!(second.trust(second_id), Err(AppError::SshHostKeyChanged));
    assert_eq!(
        first.verify("example.com", 22, &ed25519_key(7), HostKeyType::Ed25519),
        Ok(())
    );
}

#[test]
fn discarded_or_unknown_challenges_cannot_write_trust() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("known_hosts");
    let store = SshHostKeyStore::new(path.clone());
    let id = unknown_id(store.verify("example.com", 22, &ed25519_key(7), HostKeyType::Ed25519));
    store.discard(id).unwrap();
    assert_eq!(
        store.get_challenge(id),
        Err(AppError::SshHostKeyChallengeExpired)
    );
    assert_eq!(store.trust(id), Err(AppError::SshHostKeyChallengeExpired));
    assert_eq!(
        store.trust(uuid::Uuid::new_v4()),
        Err(AppError::SshHostKeyChallengeExpired)
    );
    assert!(!path.exists());
}

#[test]
fn corrupt_store_is_not_treated_as_an_unknown_host() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("known_hosts");
    std::fs::write(&path, "corrupt known hosts data\n").unwrap();
    let store = SshHostKeyStore::new(path.clone());
    assert_eq!(
        store.verify("example.com", 22, &ed25519_key(7), HostKeyType::Ed25519),
        Err(AppError::SshHostKeyStoreFailed)
    );
    assert_eq!(
        std::fs::read_to_string(path).unwrap(),
        "corrupt known hosts data\n"
    );
}

#[test]
fn ipv6_host_trust_is_scoped_to_its_exact_port() {
    let directory = tempfile::tempdir().unwrap();
    let store = SshHostKeyStore::new(directory.path().join("known_hosts"));
    let key = ed25519_key(7);
    let id = unknown_id(store.verify("::1", 2222, &key, HostKeyType::Ed25519));
    store.trust(id).unwrap();
    assert_eq!(
        store.verify("::1", 2222, &key, HostKeyType::Ed25519),
        Ok(())
    );
    unknown_id(store.verify("::1", 22, &key, HostKeyType::Ed25519));
}

#[test]
fn unknown_host_error_exposes_only_the_challenge_identifier() {
    let id = uuid::Uuid::new_v4();
    let error = serde_json::to_value(AppError::SshHostKeyUnknown(id)).unwrap();
    assert_eq!(error["code"], "SSH_HOST_KEY_UNKNOWN");
    assert_eq!(error["challenge_id"], id.to_string());
    assert_eq!(error.as_object().unwrap().len(), 3);
}

#[test]
fn a_different_key_algorithm_is_a_change_not_a_first_connection() {
    let directory = tempfile::tempdir().unwrap();
    let store = SshHostKeyStore::new(directory.path().join("known_hosts"));
    let id = unknown_id(store.verify("example.com", 22, &ed25519_key(7), HostKeyType::Ed25519));
    store.trust(id).unwrap();
    let rsa_key = b"\0\0\0\x07ssh-rsa\0\0\0\x01\x03\0\0\0\x01\x07";
    assert_eq!(
        store.verify("example.com", 22, rsa_key, HostKeyType::Rsa),
        Err(AppError::SshHostKeyChanged)
    );
}

#[test]
fn independent_stores_preserve_each_others_confirmed_hosts() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("known_hosts");
    let first = SshHostKeyStore::new(path.clone());
    let second = SshHostKeyStore::new(path);
    let key = ed25519_key(7);
    let first_id = unknown_id(first.verify("first.example", 22, &key, HostKeyType::Ed25519));
    let second_id = unknown_id(second.verify("second.example", 22, &key, HostKeyType::Ed25519));
    first.trust(first_id).unwrap();
    second.trust(second_id).unwrap();
    assert_eq!(
        second.verify("first.example", 22, &key, HostKeyType::Ed25519),
        Ok(())
    );
    assert_eq!(
        first.verify("second.example", 22, &key, HostKeyType::Ed25519),
        Ok(())
    );
}

#[test]
fn failed_persistence_never_reports_trust_success() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("known_hosts");
    let store = SshHostKeyStore::new(path.clone());
    let id = unknown_id(store.verify("example.com", 22, &ed25519_key(7), HostKeyType::Ed25519));
    std::fs::create_dir(&path).unwrap();
    assert_eq!(store.trust(id), Err(AppError::SshHostKeyStoreFailed));
    assert!(path.is_dir());
}

#[test]
fn malformed_host_fields_and_truncated_public_keys_are_store_failures() {
    use base64::{engine::general_purpose::STANDARD, Engine};
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("known_hosts");
    let store = SshHostKeyStore::new(path.clone());
    let key = ed25519_key(7);
    for line in [
        format!(
            "[example.com]:invalid ssh-ed25519 {}\n",
            STANDARD.encode(&key)
        ),
        format!(
            "[example.com]:22 ssh-ed25519 {}\n",
            STANDARD.encode(&key[..20])
        ),
    ] {
        std::fs::write(&path, line).unwrap();
        assert_eq!(
            store.verify("example.com", 22, &key, HostKeyType::Ed25519),
            Err(AppError::SshHostKeyStoreFailed)
        );
    }
}

#[test]
fn standard_openssh_host_fields_are_read_from_the_application_store() {
    use base64::{engine::general_purpose::STANDARD, Engine};
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("known_hosts");
    let store = SshHostKeyStore::new(path.clone());
    let key = ed25519_key(7);
    for host in [
        "example.com",
        "[example.com]:22",
        "|1|AQIDBAUGBwgJCgsMDQ4PEBESExQ=|ZPCaIWkIizT/3iOVVyeg848Days=",
    ] {
        std::fs::write(
            &path,
            format!("{host} ssh-ed25519 {}\n", STANDARD.encode(&key)),
        )
        .unwrap();
        assert_eq!(
            store.verify("example.com", 22, &key, HostKeyType::Ed25519),
            Ok(())
        );
    }
}

#[test]
fn supported_key_algorithms_keep_their_full_public_key_when_persisted() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("known_hosts");
    let store = SshHostKeyStore::new(path.clone());
    for (index, (key_type, algorithm, fields)) in [
        (HostKeyType::Rsa, "ssh-rsa", vec![vec![3], vec![7]]),
        (
            HostKeyType::Dss,
            "ssh-dss",
            vec![vec![1], vec![2], vec![3], vec![4]],
        ),
        (
            HostKeyType::Ecdsa256,
            "ecdsa-sha2-nistp256",
            vec![b"nistp256".to_vec(), vec![4; 65]],
        ),
        (
            HostKeyType::Ecdsa384,
            "ecdsa-sha2-nistp384",
            vec![b"nistp384".to_vec(), vec![4; 97]],
        ),
        (
            HostKeyType::Ecdsa521,
            "ecdsa-sha2-nistp521",
            vec![b"nistp521".to_vec(), vec![4; 133]],
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let mut key = Vec::new();
        for field in std::iter::once(algorithm.as_bytes().to_vec()).chain(fields) {
            key.extend_from_slice(&(field.len() as u32).to_be_bytes());
            key.extend_from_slice(&field);
        }
        let host = format!("algorithm-{index}.example");
        let id = unknown_id(store.verify(&host, 22, &key, key_type));
        assert_eq!(store.get_challenge(id).unwrap().algorithm, algorithm);
        store.trust(id).unwrap();
        assert_eq!(
            SshHostKeyStore::new(path.clone()).verify(&host, 22, &key, key_type),
            Ok(())
        );
    }
}

#[test]
fn simultaneous_confirmations_do_not_lose_other_host_records() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("known_hosts");
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(8));
    let workers = (0..8)
        .map(|index| {
            let store = SshHostKeyStore::new(path.clone());
            let id = unknown_id(store.verify(
                &format!("host-{index}.example"),
                22,
                &ed25519_key(7),
                HostKeyType::Ed25519,
            ));
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                store.trust(id).unwrap();
            })
        })
        .collect::<Vec<_>>();
    for worker in workers {
        worker.join().unwrap();
    }
    let store = SshHostKeyStore::new(path);
    for index in 0..8 {
        assert_eq!(
            store.verify(
                &format!("host-{index}.example"),
                22,
                &ed25519_key(7),
                HostKeyType::Ed25519
            ),
            Ok(())
        );
    }
}

#[test]
fn clearing_hosts_requires_fresh_confirmation_and_preserves_other_app_data() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("known_hosts");
    let unrelated = directory.path().join("connection-secrets.json");
    std::fs::write(&unrelated, "untouched").unwrap();
    let store = SshHostKeyStore::new(path.clone());
    let key = ed25519_key(7);
    for host in ["first.example", "second.example"] {
        let id = unknown_id(store.verify(host, 22, &key, HostKeyType::Ed25519));
        store.trust(id).unwrap();
    }
    let pending = unknown_id(store.verify("pending.example", 22, &key, HostKeyType::Ed25519));
    store.clear().unwrap();
    assert_eq!(
        store.get_challenge(pending),
        Err(AppError::SshHostKeyChallengeExpired)
    );
    assert_eq!(
        store.trust(pending),
        Err(AppError::SshHostKeyChallengeExpired)
    );
    assert_eq!(std::fs::read_to_string(unrelated).unwrap(), "untouched");
    let restored = SshHostKeyStore::new(path);
    for host in ["first.example", "second.example"] {
        let id = unknown_id(restored.verify(host, 22, &key, HostKeyType::Ed25519));
        restored.trust(id).unwrap();
        assert_eq!(
            restored.verify(host, 22, &key, HostKeyType::Ed25519),
            Ok(())
        );
    }
}

#[test]
fn clearing_an_empty_store_invalidates_challenges_from_other_instances() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("known_hosts");
    let clearing = SshHostKeyStore::new(path.clone());
    let other = SshHostKeyStore::new(path);
    let key = ed25519_key(7);
    for _ in 0..2 {
        let id = unknown_id(other.verify("example.com", 22, &key, HostKeyType::Ed25519));
        clearing.clear().unwrap();
        assert_eq!(other.trust(id), Err(AppError::SshHostKeyChallengeExpired));
    }
    let id = unknown_id(other.verify("example.com", 22, &key, HostKeyType::Ed25519));
    other.trust(id).unwrap();
    assert_eq!(
        clearing.verify("example.com", 22, &key, HostKeyType::Ed25519),
        Ok(())
    );
}

#[test]
fn clearing_a_corrupt_store_recovers_without_parsing_the_old_contents() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("known_hosts");
    std::fs::write(&path, "corrupt data").unwrap();
    let store = SshHostKeyStore::new(path);
    store.clear().unwrap();
    unknown_id(store.verify("example.com", 22, &ed25519_key(7), HostKeyType::Ed25519));
}

#[test]
fn failed_clear_preserves_pending_challenges_and_reports_storage_error() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("known_hosts");
    let store = SshHostKeyStore::new(path.clone());
    let id = unknown_id(store.verify("example.com", 22, &ed25519_key(7), HostKeyType::Ed25519));
    std::fs::create_dir(&path).unwrap();
    assert_eq!(store.clear(), Err(AppError::SshHostKeyStoreFailed));
    assert!(store.get_challenge(id).is_ok());
    assert!(path.is_dir());
}

#[test]
fn clearing_and_confirming_concurrently_never_restore_pre_clear_trust() {
    use std::sync::{Arc, Barrier};
    for _ in 0..8 {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("known_hosts");
        let clearing = SshHostKeyStore::new(path.clone());
        let other = SshHostKeyStore::new(path.clone());
        let key = ed25519_key(7);
        let id = unknown_id(other.verify("example.com", 22, &key, HostKeyType::Ed25519));
        let barrier = Arc::new(Barrier::new(2));
        let other_barrier = barrier.clone();
        let confirm = std::thread::spawn(move || {
            other_barrier.wait();
            other.trust(id)
        });
        barrier.wait();
        clearing.clear().unwrap();
        let result = confirm.join().unwrap();
        assert!(matches!(
            result,
            Ok(()) | Err(AppError::SshHostKeyChallengeExpired)
        ));
        let restored = SshHostKeyStore::new(path);
        unknown_id(restored.verify("example.com", 22, &key, HostKeyType::Ed25519));
    }
}
