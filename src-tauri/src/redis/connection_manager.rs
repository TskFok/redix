use std::{collections::HashMap, sync::Arc};

use ::redis::{Client, Value};
use tokio::sync::RwLock;

use crate::{
    domain::{
        normalize_key_type, parse_info_sections, parse_keyspace_line, CommandDefinition,
        CommandExecutionItem, CommandResult, ConnectionInfo, ConnectionProfile, CreateKeyInput,
        DatabaseOverview, DeleteKeysInput, ExecuteCommandsInput, ExportKeysInput, ExportedKey,
        HashEntry, ImportKeysInput, InstanceOverview, KeyInfo, KeyInfoInput, KeySummary, KeyValue,
        ModuleSummary, RedisValue, RenameKeyInput, ScanKeysInput, ScanPage, SelectDatabaseInput,
        SetKeyInput, SetKeyTtlInput, SortedSetEntry, StreamEntry,
    },
    error::AppError,
    persistence::{ProfileRepository, SecretStore},
};

use super::{
    key_ops::{decode_json_value, decode_stream_entry, encode_json_value, encode_stream_entry},
    tokenize_command,
};

#[allow(async_fn_in_trait)]
pub trait RedisOperations: Send + Sync {
    async fn test_connection(
        &self,
        profile: &ConnectionProfile,
        password: Option<&str>,
    ) -> Result<ConnectionInfo, AppError>;
    async fn open_connection(&self, connection_id: &str) -> Result<ConnectionInfo, AppError>;
    async fn close_connection(&self, connection_id: &str) -> Result<(), AppError>;
    async fn scan_keys(&self, input: crate::domain::ScanKeysInput) -> Result<ScanPage, AppError>;
    async fn get_key(&self, connection_id: &str, key: &str) -> Result<KeyValue, AppError>;
    async fn set_key(&self, input: SetKeyInput) -> Result<KeyValue, AppError>;
    async fn create_key(&self, input: CreateKeyInput) -> Result<KeyValue, AppError>;
    async fn rename_key(&self, input: RenameKeyInput) -> Result<KeyValue, AppError>;
    async fn delete_key(&self, connection_id: &str, key: &str) -> Result<(), AppError>;
    async fn delete_keys(&self, input: DeleteKeysInput) -> Result<u64, AppError>;
    async fn set_key_ttl(&self, input: SetKeyTtlInput) -> Result<i64, AppError>;
    async fn get_key_info(&self, input: KeyInfoInput) -> Result<KeyInfo, AppError>;
    async fn export_keys(&self, input: ExportKeysInput) -> Result<Vec<ExportedKey>, AppError>;
    async fn import_keys(&self, input: ImportKeysInput) -> Result<u64, AppError>;
    async fn get_instance_overview(
        &self,
        connection_id: &str,
    ) -> Result<InstanceOverview, AppError>;
    async fn get_database_overview(
        &self,
        connection_id: &str,
    ) -> Result<Vec<DatabaseOverview>, AppError>;
    async fn select_database(
        &self,
        input: SelectDatabaseInput,
    ) -> Result<ConnectionProfile, AppError>;
    async fn execute_command(
        &self,
        connection_id: &str,
        input: &str,
    ) -> Result<CommandResult, AppError>;
    async fn execute_commands(
        &self,
        input: ExecuteCommandsInput,
    ) -> Result<Vec<CommandExecutionItem>, AppError>;
    fn command_catalog(&self) -> Vec<CommandDefinition>;
}

pub struct RedisService {
    profiles: Arc<dyn ProfileRepository>,
    secrets: Arc<dyn SecretStore>,
    active: Arc<RwLock<HashMap<String, Client>>>,
}

impl RedisService {
    pub fn new(profiles: Arc<dyn ProfileRepository>, secrets: Arc<dyn SecretStore>) -> Self {
        Self {
            profiles,
            secrets,
            active: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    async fn client(&self, connection_id: &str) -> Result<Client, AppError> {
        self.active
            .read()
            .await
            .get(connection_id)
            .cloned()
            .ok_or(AppError::ConnectionFailed)
    }

    async fn connection(
        &self,
        connection_id: &str,
    ) -> Result<::redis::aio::MultiplexedConnection, AppError> {
        self.client(connection_id)
            .await?
            .get_multiplexed_async_connection()
            .await
            .map_err(map_connection_error)
    }

    async fn inspect_client(client: &Client) -> Result<ConnectionInfo, AppError> {
        let mut connection = client
            .get_multiplexed_async_connection()
            .await
            .map_err(map_connection_error)?;
        let pong: String = ::redis::cmd("PING")
            .query_async::<String>(&mut connection)
            .await
            .map_err(map_connection_error)?;
        if pong != "PONG" {
            return Err(AppError::ConnectionFailed);
        }

        let server_info: Result<String, _> = ::redis::cmd("INFO")
            .arg("server")
            .query_async(&mut connection)
            .await;
        let server_version = server_info
            .ok()
            .and_then(|info| {
                info.lines()
                    .find_map(|line| line.strip_prefix("redis_version:"))
                    .map(str::trim)
                    .filter(|version| !version.is_empty())
                    .map(str::to_owned)
            })
            .unwrap_or_else(|| "unknown".into());

        Ok(ConnectionInfo { server_version })
    }

    fn profile(&self, connection_id: &str) -> Result<ConnectionProfile, AppError> {
        self.profiles
            .load()?
            .into_iter()
            .find(|profile| profile.id == connection_id)
            .ok_or(AppError::InvalidConnection)
    }
}

impl RedisOperations for RedisService {
    async fn test_connection(
        &self,
        profile: &ConnectionProfile,
        password: Option<&str>,
    ) -> Result<ConnectionInfo, AppError> {
        profile.validate()?;
        let client = Client::open(connection_url(profile, password)?)
            .map_err(|_| AppError::InvalidConnection)?;
        Self::inspect_client(&client).await
    }

    async fn open_connection(&self, connection_id: &str) -> Result<ConnectionInfo, AppError> {
        let profile = self
            .profiles
            .load()?
            .into_iter()
            .find(|profile| profile.id == connection_id)
            .ok_or(AppError::InvalidConnection)?;
        profile.validate()?;
        let password = if profile.has_password {
            self.secrets.read(connection_id)?
        } else {
            None
        };
        let client = Client::open(connection_url(&profile, password.as_deref())?)
            .map_err(|_| AppError::InvalidConnection)?;
        let info = Self::inspect_client(&client).await?;
        self.active
            .write()
            .await
            .insert(connection_id.to_owned(), client);
        Ok(info)
    }

    async fn close_connection(&self, connection_id: &str) -> Result<(), AppError> {
        self.active.write().await.remove(connection_id);
        Ok(())
    }

    async fn scan_keys(&self, input: ScanKeysInput) -> Result<ScanPage, AppError> {
        input.validate()?;
        let requested_type = input.key_type.as_deref().and_then(normalize_key_type);
        let mut connection = self.connection(&input.connection_id).await?;
        let (cursor, keys): (u64, Vec<String>) = ::redis::cmd("SCAN")
            .arg(input.cursor)
            .arg("MATCH")
            .arg(&input.pattern)
            .arg("COUNT")
            .arg(input.count)
            .query_async::<(u64, Vec<String>)>(&mut connection)
            .await
            .map_err(map_command_error)?;
        let mut summaries = Vec::with_capacity(keys.len());

        for key in keys {
            let key_type: String = ::redis::cmd("TYPE")
                .arg(&key)
                .query_async::<String>(&mut connection)
                .await
                .map_err(map_command_error)?;
            if requested_type.is_some() && normalize_key_type(&key_type) != requested_type {
                continue;
            }
            let ttl_ms: i64 = ::redis::cmd("PTTL")
                .arg(&key)
                .query_async::<i64>(&mut connection)
                .await
                .map_err(map_command_error)?;
            let size = key_size(&mut connection, &key, &key_type)
                .await
                .ok()
                .flatten();
            let memory_bytes = ::redis::cmd("MEMORY")
                .arg("USAGE")
                .arg(&key)
                .query_async::<Option<u64>>(&mut connection)
                .await
                .ok()
                .flatten();
            let encoding = ::redis::cmd("OBJECT")
                .arg("ENCODING")
                .arg(&key)
                .query_async::<Option<String>>(&mut connection)
                .await
                .ok()
                .flatten();
            let idle_seconds = ::redis::cmd("OBJECT")
                .arg("IDLETIME")
                .arg(&key)
                .query_async::<Option<u64>>(&mut connection)
                .await
                .ok()
                .flatten();
            summaries.push(KeySummary {
                key,
                key_type,
                ttl_ms,
                size,
                memory_bytes,
                encoding,
                idle_seconds,
            });
        }

        Ok(ScanPage {
            cursor,
            keys: summaries,
            has_more: cursor != 0,
        })
    }

    async fn get_key(&self, connection_id: &str, key: &str) -> Result<KeyValue, AppError> {
        let mut connection = self.connection(connection_id).await?;
        read_key(&mut connection, key).await
    }

    async fn set_key(&self, input: SetKeyInput) -> Result<KeyValue, AppError> {
        let mut connection = self.connection(&input.connection_id).await?;
        write_key(&mut connection, &input.key, &input.value).await?;
        read_key(&mut connection, &input.key).await
    }

    async fn create_key(&self, input: CreateKeyInput) -> Result<KeyValue, AppError> {
        input.validate()?;
        let mut connection = self.connection(&input.connection_id).await?;
        let exists: i64 = ::redis::cmd("EXISTS")
            .arg(&input.key)
            .query_async::<i64>(&mut connection)
            .await
            .map_err(map_command_error)?;
        if exists > 0 {
            return Err(AppError::CommandFailed);
        }

        write_key(&mut connection, &input.key, &input.value).await?;
        if let Some(ttl_ms) = input.ttl_ms {
            apply_ttl(&mut connection, &input.key, ttl_ms).await?;
        }
        read_key(&mut connection, &input.key).await
    }

    async fn rename_key(&self, input: RenameKeyInput) -> Result<KeyValue, AppError> {
        input.validate()?;
        let mut connection = self.connection(&input.connection_id).await?;
        let renamed: i64 = ::redis::cmd("RENAMENX")
            .arg(&input.key)
            .arg(&input.new_key)
            .query_async::<i64>(&mut connection)
            .await
            .map_err(map_command_error)?;
        if renamed == 0 {
            return Err(AppError::CommandFailed);
        }
        read_key(&mut connection, &input.new_key).await
    }

    async fn delete_key(&self, connection_id: &str, key: &str) -> Result<(), AppError> {
        let mut connection = self.connection(connection_id).await?;
        ::redis::cmd("DEL")
            .arg(key)
            .query_async::<i64>(&mut connection)
            .await
            .map_err(map_command_error)?;
        Ok(())
    }

    async fn delete_keys(&self, input: DeleteKeysInput) -> Result<u64, AppError> {
        input.validate()?;
        let mut connection = self.connection(&input.connection_id).await?;
        let deleted: i64 = ::redis::cmd("DEL")
            .arg(&input.keys)
            .query_async::<i64>(&mut connection)
            .await
            .map_err(map_command_error)?;
        u64::try_from(deleted).map_err(|_| AppError::CommandFailed)
    }

    async fn set_key_ttl(&self, input: SetKeyTtlInput) -> Result<i64, AppError> {
        validate_ttl(input.ttl_ms)?;
        let mut connection = self.connection(&input.connection_id).await?;
        let updated: i64 = ::redis::cmd("PEXPIRE")
            .arg(&input.key)
            .arg(input.ttl_ms)
            .query_async::<i64>(&mut connection)
            .await
            .map_err(map_command_error)?;
        if updated == 0 {
            return Err(AppError::CommandFailed);
        }
        ::redis::cmd("PTTL")
            .arg(&input.key)
            .query_async::<i64>(&mut connection)
            .await
            .map_err(map_command_error)
    }

    async fn get_key_info(&self, input: KeyInfoInput) -> Result<KeyInfo, AppError> {
        input.validate()?;
        let mut connection = self.connection(&input.connection_id).await?;
        read_key_info(&mut connection, &input.key).await
    }

    async fn export_keys(&self, input: ExportKeysInput) -> Result<Vec<ExportedKey>, AppError> {
        input.validate()?;
        let mut connection = self.connection(&input.connection_id).await?;
        let mut exported = Vec::with_capacity(input.keys.len());
        for key in input.keys {
            let value = read_key(&mut connection, &key).await?;
            exported.push(ExportedKey {
                key,
                ttl_ms: value.ttl_ms,
                value: value.value,
            });
        }
        Ok(exported)
    }

    async fn import_keys(&self, input: ImportKeysInput) -> Result<u64, AppError> {
        input.validate()?;
        let mut connection = self.connection(&input.connection_id).await?;
        let mut imported = 0_u64;
        for entry in input.entries {
            let exists: i64 = ::redis::cmd("EXISTS")
                .arg(&entry.key)
                .query_async::<i64>(&mut connection)
                .await
                .map_err(map_command_error)?;
            if exists > 0 {
                continue;
            }

            write_key(&mut connection, &entry.key, &entry.value).await?;
            if entry.ttl_ms >= 0 {
                apply_ttl(&mut connection, &entry.key, entry.ttl_ms).await?;
            }
            imported += 1;
        }
        Ok(imported)
    }

    async fn get_instance_overview(
        &self,
        connection_id: &str,
    ) -> Result<InstanceOverview, AppError> {
        let mut connection = self.connection(connection_id).await?;
        let info = ::redis::cmd("INFO")
            .query_async::<String>(&mut connection)
            .await
            .unwrap_or_default();
        let sections = parse_info_sections(&info);

        let modules = ::redis::cmd("MODULE")
            .arg("LIST")
            .query_async::<Value>(&mut connection)
            .await
            .map(parse_module_list)
            .unwrap_or_default();

        InstanceOverview::from_info_and_modules(&sections, modules)
    }

    async fn get_database_overview(
        &self,
        connection_id: &str,
    ) -> Result<Vec<DatabaseOverview>, AppError> {
        let profile = self.profile(connection_id)?;
        profile.validate()?;
        let mut connection = self.connection(connection_id).await?;
        let info = ::redis::cmd("INFO")
            .arg("keyspace")
            .query_async::<String>(&mut connection)
            .await;

        if let Ok(info) = info {
            let sections = parse_info_sections(&info);
            let mut databases = sections
                .get("Keyspace")
                .map(|entries| {
                    entries
                        .iter()
                        .filter(|(database, _)| database.starts_with("db"))
                        .map(|(database, line)| parse_keyspace_line(database, line))
                        .collect::<Result<Vec<_>, _>>()
                })
                .transpose()?
                .unwrap_or_default();
            databases.sort_by_key(|database| database.database);
            return Ok(databases);
        }

        let key_count: u64 = ::redis::cmd("DBSIZE")
            .query_async::<u64>(&mut connection)
            .await
            .map_err(map_command_error)?;
        Ok(vec![DatabaseOverview {
            database: profile.database,
            key_count: Some(key_count),
            expires: None,
            avg_ttl_ms: None,
        }])
    }

    async fn select_database(
        &self,
        input: SelectDatabaseInput,
    ) -> Result<ConnectionProfile, AppError> {
        input.validate()?;
        let old_profiles = self.profiles.load()?;
        let old_profile = old_profiles
            .iter()
            .find(|profile| profile.id == input.connection_id)
            .cloned()
            .ok_or(AppError::InvalidConnection)?;
        old_profile.validate()?;
        let password = if old_profile.has_password {
            self.secrets.read(&input.connection_id)?
        } else {
            None
        };
        let mut new_profile = old_profile.clone();
        new_profile.database = input.database;
        let client = Client::open(connection_url_with_database(
            &old_profile,
            password.as_deref(),
            input.database,
        )?)
        .map_err(|_| AppError::InvalidConnection)?;
        Self::inspect_client(&client).await?;

        let mut profiles = old_profiles.clone();
        let profile = profiles
            .iter_mut()
            .find(|profile| profile.id == input.connection_id)
            .ok_or(AppError::InvalidConnection)?;
        *profile = new_profile.clone();
        if self.profiles.save(&profiles).is_err() {
            let _ = self.profiles.save(&old_profiles);
            return Err(AppError::PersistenceFailed);
        }

        self.active
            .write()
            .await
            .insert(input.connection_id, client);
        Ok(new_profile)
    }

    async fn execute_command(
        &self,
        connection_id: &str,
        input: &str,
    ) -> Result<CommandResult, AppError> {
        let mut connection = self.connection(connection_id).await?;
        execute_tokenized_command(&mut connection, input).await
    }

    async fn execute_commands(
        &self,
        input: ExecuteCommandsInput,
    ) -> Result<Vec<CommandExecutionItem>, AppError> {
        input.validate()?;
        let mut connection = self.connection(&input.connection_id).await?;
        let mut items = Vec::with_capacity(input.commands.len());
        for raw_command in input.commands {
            let command = raw_command.trim().to_owned();
            match execute_tokenized_command(&mut connection, &command).await {
                Ok(result) => items.push(CommandExecutionItem {
                    command,
                    result: Some(result),
                    error_code: None,
                }),
                Err(error) => {
                    items.push(CommandExecutionItem {
                        command,
                        result: None,
                        error_code: Some(error.code().to_owned()),
                    });
                    if !input.continue_on_error {
                        break;
                    }
                }
            }
        }
        Ok(items)
    }

    fn command_catalog(&self) -> Vec<CommandDefinition> {
        crate::domain::command_catalog()
    }
}

async fn execute_tokenized_command(
    connection: &mut ::redis::aio::MultiplexedConnection,
    input: &str,
) -> Result<CommandResult, AppError> {
    let arguments = tokenize_command(input)?;
    let mut command = ::redis::cmd(&arguments[0]);
    command.arg(&arguments[1..]);
    let value: Value = command
        .query_async::<Value>(connection)
        .await
        .map_err(map_command_error)?;
    command_result(value)
}

async fn key_size(
    connection: &mut ::redis::aio::MultiplexedConnection,
    key: &str,
    key_type: &str,
) -> Result<Option<u64>, AppError> {
    let command = match key_type {
        "string" => "STRLEN",
        "hash" => "HLEN",
        "list" => "LLEN",
        "set" => "SCARD",
        "zset" => "ZCARD",
        "stream" => "XLEN",
        "ReJSON-RL" | "ReJSON-RS" | "JSON" => return Ok(None),
        _ => return Ok(None),
    };
    ::redis::cmd(command)
        .arg(key)
        .query_async::<u64>(connection)
        .await
        .map(Some)
        .map_err(map_command_error)
}

async fn apply_ttl(
    connection: &mut ::redis::aio::MultiplexedConnection,
    key: &str,
    ttl_ms: i64,
) -> Result<i64, AppError> {
    validate_ttl(ttl_ms)?;
    let updated: i64 = ::redis::cmd("PEXPIRE")
        .arg(key)
        .arg(ttl_ms)
        .query_async::<i64>(connection)
        .await
        .map_err(map_command_error)?;
    if updated == 0 {
        return Err(AppError::CommandFailed);
    }
    ::redis::cmd("PTTL")
        .arg(key)
        .query_async::<i64>(connection)
        .await
        .map_err(map_command_error)
}

async fn read_key_info(
    connection: &mut ::redis::aio::MultiplexedConnection,
    key: &str,
) -> Result<KeyInfo, AppError> {
    let key_type: String = ::redis::cmd("TYPE")
        .arg(key)
        .query_async::<String>(connection)
        .await
        .map_err(map_command_error)?;
    if key_type == "none" {
        return Err(AppError::CommandFailed);
    }
    let ttl_ms: i64 = ::redis::cmd("PTTL")
        .arg(key)
        .query_async::<i64>(connection)
        .await
        .map_err(map_command_error)?;
    let size = key_size(connection, key, &key_type).await?;
    let memory_bytes = ::redis::cmd("MEMORY")
        .arg("USAGE")
        .arg(key)
        .query_async::<Option<u64>>(connection)
        .await
        .ok()
        .flatten();
    let encoding = ::redis::cmd("OBJECT")
        .arg("ENCODING")
        .arg(key)
        .query_async::<Option<String>>(connection)
        .await
        .ok()
        .flatten();
    let idle_seconds = ::redis::cmd("OBJECT")
        .arg("IDLETIME")
        .arg(key)
        .query_async::<Option<u64>>(connection)
        .await
        .ok()
        .flatten();

    Ok(KeyInfo {
        key: key.to_owned(),
        key_type,
        ttl_ms,
        size,
        memory_bytes,
        encoding,
        idle_seconds,
    })
}

async fn read_key(
    connection: &mut ::redis::aio::MultiplexedConnection,
    key: &str,
) -> Result<KeyValue, AppError> {
    let key_type: String = ::redis::cmd("TYPE")
        .arg(key)
        .query_async::<String>(connection)
        .await
        .map_err(map_command_error)?;
    if key_type == "none" {
        return Err(AppError::CommandFailed);
    }

    let value = match key_type.as_str() {
        "string" => RedisValue::String {
            value: ::redis::cmd("GET")
                .arg(key)
                .query_async::<String>(connection)
                .await
                .map_err(map_command_error)?,
        },
        "hash" => RedisValue::Hash {
            fields: ::redis::cmd("HGETALL")
                .arg(key)
                .query_async::<Vec<(String, String)>>(connection)
                .await
                .map_err(map_command_error)?
                .into_iter()
                .map(|(field, value)| HashEntry { field, value })
                .collect(),
        },
        "list" => RedisValue::List {
            items: ::redis::cmd("LRANGE")
                .arg(key)
                .arg(0)
                .arg(-1)
                .query_async::<Vec<String>>(connection)
                .await
                .map_err(map_command_error)?,
        },
        "set" => RedisValue::Set {
            members: ::redis::cmd("SMEMBERS")
                .arg(key)
                .query_async::<Vec<String>>(connection)
                .await
                .map_err(map_command_error)?,
        },
        "zset" => RedisValue::SortedSet {
            members: ::redis::cmd("ZRANGE")
                .arg(key)
                .arg(0)
                .arg(-1)
                .arg("WITHSCORES")
                .query_async::<Vec<(String, f64)>>(connection)
                .await
                .map_err(map_command_error)?
                .into_iter()
                .map(|(member, score)| SortedSetEntry { member, score })
                .collect(),
        },
        "stream" => {
            let reply: ::redis::streams::StreamRangeReply = ::redis::cmd("XRANGE")
                .arg(key)
                .arg("-")
                .arg("+")
                .arg("COUNT")
                .arg(500)
                .query_async::<::redis::streams::StreamRangeReply>(connection)
                .await
                .map_err(map_command_error)?;
            let entries = reply
                .ids
                .into_iter()
                .map(stream_entry_from_reply)
                .collect::<Result<Vec<_>, AppError>>()?;
            RedisValue::Stream { entries }
        }
        "ReJSON-RL" | "ReJSON-RS" | "JSON" => {
            let raw: String = ::redis::cmd("JSON.GET")
                .arg(key)
                .arg(".")
                .query_async::<String>(connection)
                .await
                .map_err(map_json_command_error)?;
            RedisValue::Json {
                value: decode_json_value(&raw)?,
            }
        }
        _ => return Err(AppError::UnsupportedDataType),
    };
    let ttl_ms = ::redis::cmd("PTTL")
        .arg(key)
        .query_async::<i64>(connection)
        .await
        .map_err(map_command_error)?;
    Ok(KeyValue {
        key: key.to_owned(),
        key_type,
        ttl_ms,
        value,
    })
}

fn stream_entry_from_reply(entry: ::redis::streams::StreamId) -> Result<StreamEntry, AppError> {
    let id = entry.id;
    let mut fields = Vec::with_capacity(entry.map.len());
    for (field, value) in entry.map {
        let value =
            ::redis::from_redis_value::<String>(value).map_err(|_| AppError::CommandFailed)?;
        fields.push((field, value));
    }
    fields.sort_by(|left, right| left.0.cmp(&right.0));
    decode_stream_entry(
        &id,
        fields
            .into_iter()
            .flat_map(|(field, value)| [field, value])
            .collect(),
    )
}

async fn write_key(
    connection: &mut ::redis::aio::MultiplexedConnection,
    key: &str,
    value: &RedisValue,
) -> Result<(), AppError> {
    value.validate()?;
    match value {
        RedisValue::String { value } => {
            ::redis::cmd("SET")
                .arg(key)
                .arg(value)
                .query_async::<String>(connection)
                .await
                .map_err(map_command_error)?;
        }
        RedisValue::Hash { fields } => {
            if fields.is_empty() {
                return Err(AppError::CommandFailed);
            }
            replace_collection(connection, key).await?;
            let mut command = ::redis::cmd("HSET");
            command.arg(key);
            for entry in fields {
                command.arg(&entry.field).arg(&entry.value);
            }
            command
                .query_async::<i64>(connection)
                .await
                .map_err(map_command_error)?;
        }
        RedisValue::List { items } => {
            if items.is_empty() {
                return Err(AppError::CommandFailed);
            }
            replace_collection(connection, key).await?;
            let mut command = ::redis::cmd("RPUSH");
            command.arg(key);
            for item in items {
                command.arg(item);
            }
            command
                .query_async::<i64>(connection)
                .await
                .map_err(map_command_error)?;
        }
        RedisValue::Set { members } => {
            if members.is_empty() {
                return Err(AppError::CommandFailed);
            }
            replace_collection(connection, key).await?;
            let mut command = ::redis::cmd("SADD");
            command.arg(key);
            for member in members {
                command.arg(member);
            }
            command
                .query_async::<i64>(connection)
                .await
                .map_err(map_command_error)?;
        }
        RedisValue::SortedSet { members } => {
            replace_collection(connection, key).await?;
            let mut command = ::redis::cmd("ZADD");
            command.arg(key);
            for entry in members {
                command.arg(entry.score).arg(&entry.member);
            }
            command
                .query_async::<i64>(connection)
                .await
                .map_err(map_command_error)?;
        }
        RedisValue::Json { value } => {
            let encoded = encode_json_value(value)?;
            ::redis::cmd("JSON.SET")
                .arg(key)
                .arg(".")
                .arg(encoded)
                .query_async::<String>(connection)
                .await
                .map_err(map_json_command_error)?;
        }
        RedisValue::Stream { entries } => {
            replace_collection(connection, key).await?;
            for entry in entries {
                let encoded = encode_stream_entry(entry)?;
                ::redis::cmd("XADD")
                    .arg(key)
                    .arg(&encoded)
                    .query_async::<String>(connection)
                    .await
                    .map_err(map_command_error)?;
            }
        }
    }
    Ok(())
}

async fn replace_collection(
    connection: &mut ::redis::aio::MultiplexedConnection,
    key: &str,
) -> Result<(), AppError> {
    ::redis::cmd("DEL")
        .arg(key)
        .query_async::<i64>(connection)
        .await
        .map(|_| ())
        .map_err(map_command_error)
}

fn command_result(value: Value) -> Result<CommandResult, AppError> {
    let kind = match &value {
        Value::Nil => "null",
        Value::Int(_) | Value::Double(_) => "number",
        Value::Boolean(_) => "boolean",
        Value::Array(_) | Value::Set(_) | Value::Push { .. } => "array",
        Value::Map(_) => "array",
        Value::BulkString(_)
        | Value::SimpleString(_)
        | Value::Okay
        | Value::VerbatimString { .. }
        | Value::BigNumber(_) => "string",
        Value::Attribute { data, .. } => return command_result(*data.clone()),
        Value::ServerError(_) => return Err(AppError::CommandFailed),
        _ => "unknown",
    }
    .to_owned();
    Ok(CommandResult {
        kind,
        value: value_to_json(value)?,
    })
}

fn value_to_json(value: Value) -> Result<serde_json::Value, AppError> {
    match value {
        Value::Nil => Ok(serde_json::Value::Null),
        Value::Int(value) => Ok(value.into()),
        Value::Double(value) => serde_json::Number::from_f64(value)
            .map(serde_json::Value::Number)
            .ok_or(AppError::CommandFailed),
        Value::Boolean(value) => Ok(value.into()),
        Value::BulkString(value) => Ok(String::from_utf8_lossy(&value).into_owned().into()),
        Value::SimpleString(value) => Ok(value.into()),
        Value::Okay => Ok("OK".into()),
        Value::Array(values) | Value::Set(values) => values
            .into_iter()
            .map(value_to_json)
            .collect::<Result<Vec<_>, _>>()
            .map(serde_json::Value::Array),
        Value::Map(entries) => entries
            .into_iter()
            .map(|(key, value)| {
                Ok(serde_json::json!([
                    value_to_json(key)?,
                    value_to_json(value)?
                ]))
            })
            .collect::<Result<Vec<_>, AppError>>()
            .map(serde_json::Value::Array),
        Value::Attribute { data, .. } => value_to_json(*data),
        Value::VerbatimString { text, .. } => Ok(text.into()),
        Value::BigNumber(value) => Ok(value.to_string().into()),
        Value::Push { data, .. } => data
            .into_iter()
            .map(value_to_json)
            .collect::<Result<Vec<_>, _>>()
            .map(serde_json::Value::Array),
        Value::ServerError(_) => Err(AppError::CommandFailed),
        _ => Err(AppError::CommandFailed),
    }
}

pub fn connection_url(
    profile: &ConnectionProfile,
    password: Option<&str>,
) -> Result<String, AppError> {
    connection_url_with_database(profile, password, profile.database)
}

pub fn connection_url_with_database(
    profile: &ConnectionProfile,
    password: Option<&str>,
    database: u8,
) -> Result<String, AppError> {
    profile.validate()?;
    if database > 15 {
        return Err(AppError::InvalidConnection);
    }
    let host = standalone_host(&profile.host)?;
    let credentials = match (profile.username.as_deref(), password) {
        (Some(username), Some(password)) => {
            format!("{}:{}@", percent_encode(username), percent_encode(password))
        }
        (Some(username), None) => format!("{}@", percent_encode(username)),
        (None, Some(password)) => format!(":{}@", percent_encode(password)),
        (None, None) => String::new(),
    };
    Ok(format!(
        "redis://{credentials}{host}:{}/{}",
        profile.port, database
    ))
}

fn parse_module_list(value: Value) -> Vec<ModuleSummary> {
    let entries = match value {
        Value::Array(entries) | Value::Set(entries) => entries,
        Value::Attribute { data, .. } => return parse_module_list(*data),
        _ => return Vec::new(),
    };

    entries.into_iter().filter_map(parse_module_entry).collect()
}

fn parse_module_entry(value: Value) -> Option<ModuleSummary> {
    let mut name = None;
    let mut version = None;
    match value {
        Value::Map(entries) => {
            for (key, value) in entries {
                update_module_field(&key, &value, &mut name, &mut version);
            }
        }
        Value::Array(entries) => {
            let mut pairs = entries.chunks_exact(2);
            for pair in &mut pairs {
                update_module_field(&pair[0], &pair[1], &mut name, &mut version);
            }
        }
        _ => return None,
    }

    name.filter(|name| !name.trim().is_empty())
        .map(|name| ModuleSummary {
            name,
            version: version.filter(|version| !version.trim().is_empty()),
        })
}

fn update_module_field(
    key: &Value,
    value: &Value,
    name: &mut Option<String>,
    version: &mut Option<String>,
) {
    let Some(key) = ::redis::from_redis_value_ref::<String>(key).ok() else {
        return;
    };
    match key.as_str() {
        "name" => *name = ::redis::from_redis_value_ref::<String>(value).ok(),
        "ver" | "version" => *version = ::redis::from_redis_value_ref::<String>(value).ok(),
        _ => {}
    }
}

pub fn validate_ttl(ttl_ms: i64) -> Result<(), AppError> {
    if ttl_ms < 0 {
        Err(AppError::CommandFailed)
    } else {
        Ok(())
    }
}

fn standalone_host(host: &str) -> Result<String, AppError> {
    let host = host.trim();
    if host.parse::<std::net::Ipv6Addr>().is_ok() {
        return Ok(format!("[{host}]"));
    }
    if host.is_empty()
        || host
            .bytes()
            .any(|byte| !(byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_')))
    {
        return Err(AppError::InvalidConnection);
    }
    Ok(host.to_owned())
}

fn percent_encode(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            encoded.push(byte as char);
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

fn map_connection_error(error: ::redis::RedisError) -> AppError {
    if error.kind() == ::redis::ErrorKind::AuthenticationFailed {
        AppError::AuthenticationFailed
    } else {
        AppError::ConnectionFailed
    }
}

fn map_command_error(error: ::redis::RedisError) -> AppError {
    match error.kind() {
        ::redis::ErrorKind::AuthenticationFailed => AppError::AuthenticationFailed,
        ::redis::ErrorKind::Io => AppError::ConnectionFailed,
        _ => AppError::CommandFailed,
    }
}

fn map_json_command_error(error: ::redis::RedisError) -> AppError {
    let unsupported = error.detail().is_some_and(|detail| {
        let detail = detail.to_ascii_lowercase();
        detail.contains("unknown command")
            || detail.contains("unknown subcommand")
            || detail.contains("module command")
    });
    if unsupported {
        AppError::UnsupportedDataType
    } else {
        map_command_error(error)
    }
}

#[cfg(test)]
mod tests {
    use crate::{domain::ConnectionProfile, error::AppError};

    use super::{command_result, connection_url, validate_ttl};

    fn valid_profile() -> ConnectionProfile {
        ConnectionProfile {
            id: "local".into(),
            name: "Local".into(),
            host: "127.0.0.1".into(),
            port: 6379,
            username: None,
            database: 0,
            has_password: false,
        }
    }

    #[test]
    fn builds_a_standalone_url_with_encoded_credentials_and_database() {
        let mut profile = valid_profile();
        profile.username = Some("user name".into());
        profile.database = 3;

        let url = connection_url(&profile, Some("p@ss word")).unwrap();

        assert_eq!(url, "redis://user%20name:p%40ss%20word@127.0.0.1:6379/3");
    }

    #[test]
    fn rejects_negative_ttl_without_exposing_the_value() {
        let error = validate_ttl(-1).unwrap_err();

        assert_eq!(error, AppError::CommandFailed);
        assert_eq!(error.to_string(), "Redis 命令执行失败");
    }

    #[test]
    fn serializes_redis_map_as_pair_array_with_array_kind() {
        let result = command_result(::redis::Value::Map(vec![(
            ::redis::Value::SimpleString("field".into()),
            ::redis::Value::Int(1),
        )]))
        .unwrap();

        assert_eq!(result.kind, "array");
        assert_eq!(result.value, serde_json::json!([["field", 1]]));
    }
}
