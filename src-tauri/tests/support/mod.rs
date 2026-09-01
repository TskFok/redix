use redix_lib::domain::ConnectionProfile;

pub fn valid_profile() -> ConnectionProfile {
    ConnectionProfile {
        ssh: None,
        sentinel: None,
        cluster: None,
        id: "local".into(),
        name: "Local".into(),
        host: "127.0.0.1".into(),
        port: 6379,
        username: None,
        database: 0,
        has_password: false,
        tls: false,
        verify_server_cert: true,
        ca_certificate_name: None,
        client_certificate_name: None,
        has_ca_certificate: false,
        has_client_certificate: false,
    }
}

pub fn invalid_profile() -> ConnectionProfile {
    ConnectionProfile {
        host: "".into(),
        port: 0,
        database: 16,
        ..valid_profile()
    }
}
