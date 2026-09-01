use ::redis::Value;

use crate::{
    domain::{
        ClusterNode, ClusterNodeHealth, ClusterNodeMetrics, ClusterNodeRole, ClusterSummary,
        ConnectionEndpoint, SlotRange,
    },
    error::AppError,
};

const MAX_TOPOLOGY_BYTES: usize = 4 * 1024 * 1024;
const MAX_NODES: usize = 128;
const MAX_SLOT_RANGES: usize = 16_384;
const MAX_ID_OR_HOST_BYTES: usize = 256;

pub fn parse_cluster_info(input: &str) -> Result<ClusterSummary, AppError> {
    ensure_text_size(input)?;
    let mut summary = ClusterSummary {
        state: "unknown".to_owned(),
        slots_assigned: 0,
        slots_ok: 0,
        slots_pfail: 0,
        slots_fail: 0,
        current_epoch: 0,
        size: 0,
        known_nodes: 0,
    };

    for line in input.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        match key {
            "cluster_state" => summary.state = bounded_text(value.trim())?.to_owned(),
            "cluster_slots_assigned" => summary.slots_assigned = parse_u16(value)?,
            "cluster_slots_ok" => summary.slots_ok = parse_u16(value)?,
            "cluster_slots_pfail" => summary.slots_pfail = parse_u16(value)?,
            "cluster_slots_fail" => summary.slots_fail = parse_u16(value)?,
            "cluster_current_epoch" => summary.current_epoch = parse_u64(value)?,
            "cluster_size" => summary.size = parse_u16(value)?,
            "cluster_known_nodes" => summary.known_nodes = parse_u16(value)?,
            _ => {}
        }
    }
    Ok(summary)
}

pub fn parse_cluster_shards(value: Value) -> Result<Vec<ClusterNode>, AppError> {
    ensure_value_size(&value)?;
    if let Value::ServerError(error) = value_without_attributes(&value) {
        return if is_unsupported_command(error.details().unwrap_or_default()) {
            Err(AppError::UnsupportedFeature)
        } else {
            Err(AppError::ClusterTopologyFailed)
        };
    }
    let shards =
        sequence(value_without_attributes(&value)).ok_or(AppError::ClusterTopologyFailed)?;
    let mut nodes = Vec::new();
    let mut slot_range_count = 0;
    for shard in shards {
        parse_shard(shard, &mut nodes, &mut slot_range_count)?;
    }
    Ok(nodes)
}

pub fn parse_cluster_nodes(input: &str) -> Result<Vec<ClusterNode>, AppError> {
    ensure_text_size(input)?;
    let mut nodes = Vec::new();
    let mut slot_range_count = 0;
    for line in input.lines().filter(|line| !line.trim().is_empty()) {
        if nodes.len() == MAX_NODES {
            return Err(AppError::ClusterTopologyFailed);
        }
        let node = parse_cluster_node_line(line, MAX_SLOT_RANGES - slot_range_count)?;
        slot_range_count += node.slots.len();
        nodes.push(node);
    }
    Ok(nodes)
}

pub fn merge_node_metrics(node: &mut ClusterNode, input: &str) -> Result<(), AppError> {
    ensure_text_size(input)?;
    let mut hits = None;
    let mut misses = None;
    for line in input.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        match key {
            "used_memory" => node.metrics.used_memory_bytes = Some(parse_u64(value)?),
            "instantaneous_ops_per_sec" => node.metrics.ops_per_second = Some(parse_u64(value)?),
            "total_connections_received" => {
                node.metrics.connections_received = Some(parse_u64(value)?)
            }
            "connected_clients" => node.metrics.connected_clients = Some(parse_u64(value)?),
            "total_commands_processed" => node.metrics.commands_processed = Some(parse_u64(value)?),
            "instantaneous_input_kbps" => node.metrics.network_in_kbps = Some(parse_f64(value)?),
            "instantaneous_output_kbps" => node.metrics.network_out_kbps = Some(parse_f64(value)?),
            "master_repl_offset" | "slave_repl_offset" => {
                node.metrics.replication_offset = Some(parse_u64(value)?)
            }
            "master_last_io_seconds_ago" => node.metrics.replication_lag = Some(parse_u64(value)?),
            "uptime_in_seconds" => node.metrics.uptime_seconds = Some(parse_u64(value)?),
            "keyspace_hits" => hits = Some(parse_u64(value)?),
            "keyspace_misses" => misses = Some(parse_u64(value)?),
            _ => {}
        }
    }
    if let (Some(hits), Some(misses)) = (hits, misses) {
        let total = hits.saturating_add(misses);
        if total != 0 {
            node.metrics.cache_hit_ratio = Some(hits as f64 * 100.0 / total as f64);
        }
    }
    Ok(())
}

fn parse_shard(
    shard: &Value,
    output: &mut Vec<ClusterNode>,
    slot_range_count: &mut usize,
) -> Result<(), AppError> {
    let mut slots = None;
    let mut shard_nodes = None;
    for_each_pair(shard, |key, value| match value_as_text(key) {
        Some("slots") => slots = Some(value),
        Some("nodes") => shard_nodes = Some(value),
        _ => {}
    })?;
    let slots = parse_slot_ranges(slots.ok_or(AppError::ClusterTopologyFailed)?)?;
    if slots.len() > MAX_SLOT_RANGES - *slot_range_count {
        return Err(AppError::ClusterTopologyFailed);
    }
    *slot_range_count += slots.len();
    let shard_nodes = sequence(value_without_attributes(
        shard_nodes.ok_or(AppError::ClusterTopologyFailed)?,
    ))
    .ok_or(AppError::ClusterTopologyFailed)?;
    if shard_nodes.len() > MAX_NODES.saturating_sub(output.len()) {
        return Err(AppError::ClusterTopologyFailed);
    }
    for value in shard_nodes {
        let mut node = parse_shard_node(value)?;
        if node.role == ClusterNodeRole::Primary {
            node.slots = slots.clone();
        }
        output.push(node);
    }
    Ok(())
}

fn parse_shard_node(value: &Value) -> Result<ClusterNode, AppError> {
    let mut id = None;
    let mut endpoint = None;
    let mut ip = None;
    let mut port = None;
    let mut tls_port = None;
    let mut role = None;
    let mut health = None;
    let mut primary_id = None;

    for_each_pair(value, |key, value| {
        let Some(key) = value_as_text(key) else {
            return;
        };
        match key {
            "id" => id = value_as_text(value),
            "endpoint" => endpoint = value_as_text(value),
            "ip" => ip = value_as_text(value),
            "port" => port = value_as_u16(value),
            "tls-port" => tls_port = value_as_u16(value),
            "role" => role = value_as_text(value),
            "health" => health = value_as_text(value),
            "master-id" => primary_id = value_as_text(value),
            _ => {}
        }
    })?;

    let id = bounded_text(id.ok_or(AppError::ClusterTopologyFailed)?)?.to_owned();
    let host = endpoint
        .filter(|value| !value.is_empty() && *value != "?")
        .or(ip.filter(|value| !value.is_empty()))
        .ok_or(AppError::ClusterTopologyFailed)?;
    let host = bounded_text(host)?.to_owned();
    let port = tls_port
        .filter(|port| *port != 0)
        .or(port)
        .filter(|port| *port != 0);
    let endpoint = ConnectionEndpoint {
        host,
        port: port.ok_or(AppError::ClusterTopologyFailed)?,
    };
    let role = parse_role(role.ok_or(AppError::ClusterTopologyFailed)?)?;
    let health = parse_health(health.unwrap_or("offline"));
    let primary_id = primary_id
        .filter(|value| !value.is_empty() && *value != "-")
        .map(bounded_text)
        .transpose()?
        .map(ToOwned::to_owned);

    Ok(ClusterNode {
        id,
        endpoint,
        connection_endpoint: None,
        role,
        health,
        primary_id,
        slots: Vec::new(),
        metrics: ClusterNodeMetrics::default(),
    })
}

fn parse_cluster_node_line(
    line: &str,
    remaining_slot_ranges: usize,
) -> Result<ClusterNode, AppError> {
    let mut fields = line.split_ascii_whitespace();
    let id = bounded_text(fields.next().ok_or(AppError::ClusterTopologyFailed)?)?.to_owned();
    let endpoint = parse_nodes_endpoint(fields.next().ok_or(AppError::ClusterTopologyFailed)?)?;
    let flags = fields.next().ok_or(AppError::ClusterTopologyFailed)?;
    let primary_id = fields.next().ok_or(AppError::ClusterTopologyFailed)?;
    for _ in 0..4 {
        fields.next().ok_or(AppError::ClusterTopologyFailed)?;
    }

    let role = if flags.split(',').any(|flag| flag == "master") {
        ClusterNodeRole::Primary
    } else if flags
        .split(',')
        .any(|flag| matches!(flag, "slave" | "replica"))
    {
        ClusterNodeRole::Replica
    } else {
        return Err(AppError::ClusterTopologyFailed);
    };
    let health = if flags
        .split(',')
        .any(|flag| matches!(flag, "fail" | "fail?" | "handshake" | "noaddr"))
    {
        ClusterNodeHealth::Offline
    } else if flags.split(',').any(|flag| flag == "loading") {
        ClusterNodeHealth::Loading
    } else {
        ClusterNodeHealth::Online
    };
    let primary_id = if primary_id == "-" {
        None
    } else {
        Some(bounded_text(primary_id)?.to_owned())
    };
    let mut slots = Vec::new();
    for field in fields {
        let Some(range) = parse_nodes_slot_range(field)? else {
            continue;
        };
        if slots.len() == remaining_slot_ranges {
            return Err(AppError::ClusterTopologyFailed);
        }
        slots.push(range);
    }
    Ok(ClusterNode {
        id,
        endpoint,
        connection_endpoint: None,
        role,
        health,
        primary_id,
        slots,
        metrics: ClusterNodeMetrics::default(),
    })
}

fn parse_nodes_endpoint(value: &str) -> Result<ConnectionEndpoint, AppError> {
    let address = value
        .split('@')
        .next()
        .ok_or(AppError::ClusterTopologyFailed)?;
    let (host, port) = if let Some(rest) = address.strip_prefix('[') {
        let (host, port) = rest
            .split_once("]:")
            .ok_or(AppError::ClusterTopologyFailed)?;
        if port.contains(']') {
            return Err(AppError::ClusterTopologyFailed);
        }
        (host, port)
    } else {
        address
            .rsplit_once(':')
            .ok_or(AppError::ClusterTopologyFailed)?
    };
    Ok(ConnectionEndpoint {
        host: bounded_text(host)?.to_owned(),
        port: parse_u16(port)?,
    })
}

fn parse_slot_ranges(value: &Value) -> Result<Vec<SlotRange>, AppError> {
    let values =
        sequence(value_without_attributes(value)).ok_or(AppError::ClusterTopologyFailed)?;
    if values.len() % 2 != 0 || values.len() / 2 > MAX_SLOT_RANGES {
        return Err(AppError::ClusterTopologyFailed);
    }
    let mut ranges = Vec::with_capacity(values.len() / 2);
    for pair in values.chunks_exact(2) {
        ranges.push(valid_slot_range(
            value_as_u16(&pair[0]).ok_or(AppError::ClusterTopologyFailed)?,
            value_as_u16(&pair[1]).ok_or(AppError::ClusterTopologyFailed)?,
        )?);
    }
    Ok(ranges)
}

fn parse_nodes_slot_range(value: &str) -> Result<Option<SlotRange>, AppError> {
    if value.starts_with('[') {
        return Ok(None);
    }
    let (start, end) = match value.split_once('-') {
        Some((start, end)) => (parse_u16(start)?, parse_u16(end)?),
        None => {
            if !value.bytes().all(|byte| byte.is_ascii_digit()) {
                return Ok(None);
            }
            let slot = parse_u16(value)?;
            (slot, slot)
        }
    };
    Ok(Some(valid_slot_range(start, end)?))
}

fn valid_slot_range(start: u16, end: u16) -> Result<SlotRange, AppError> {
    if start > end || end > 16_383 {
        return Err(AppError::ClusterTopologyFailed);
    }
    Ok(SlotRange { start, end })
}

fn parse_role(value: &str) -> Result<ClusterNodeRole, AppError> {
    match value {
        "master" | "primary" => Ok(ClusterNodeRole::Primary),
        "slave" | "replica" => Ok(ClusterNodeRole::Replica),
        _ => Err(AppError::ClusterTopologyFailed),
    }
}

fn parse_health(value: &str) -> ClusterNodeHealth {
    match value {
        "online" => ClusterNodeHealth::Online,
        "loading" => ClusterNodeHealth::Loading,
        _ => ClusterNodeHealth::Offline,
    }
}

fn is_unsupported_command(message: &str) -> bool {
    let message = message.to_ascii_lowercase();
    message.contains("unknown command") || message.contains("unknown subcommand")
}

fn for_each_pair<'a>(
    value: &'a Value,
    mut callback: impl FnMut(&'a Value, &'a Value),
) -> Result<(), AppError> {
    match value_without_attributes(value) {
        Value::Map(entries) => {
            for (key, value) in entries {
                callback(key, value);
            }
            Ok(())
        }
        Value::Array(values) | Value::Set(values) if values.len() % 2 == 0 => {
            for pair in values.chunks_exact(2) {
                callback(&pair[0], &pair[1]);
            }
            Ok(())
        }
        _ => Err(AppError::ClusterTopologyFailed),
    }
}

fn sequence(value: &Value) -> Option<&[Value]> {
    match value {
        Value::Array(values) | Value::Set(values) => Some(values),
        Value::Push { data, .. } => Some(data),
        _ => None,
    }
}

fn value_without_attributes(mut value: &Value) -> &Value {
    while let Value::Attribute { data, .. } = value {
        value = data;
    }
    value
}

fn value_as_text(value: &Value) -> Option<&str> {
    match value_without_attributes(value) {
        Value::BulkString(bytes) => std::str::from_utf8(bytes).ok(),
        Value::SimpleString(value) | Value::VerbatimString { text: value, .. } => Some(value),
        _ => None,
    }
}

fn value_as_u16(value: &Value) -> Option<u16> {
    match value_without_attributes(value) {
        Value::Int(value) => u16::try_from(*value).ok(),
        value => value_as_text(value).and_then(|value| value.trim().parse().ok()),
    }
}

fn ensure_text_size(value: &str) -> Result<(), AppError> {
    if value.len() > MAX_TOPOLOGY_BYTES {
        Err(AppError::ClusterTopologyFailed)
    } else {
        Ok(())
    }
}

fn ensure_value_size(root: &Value) -> Result<(), AppError> {
    let mut remaining = MAX_TOPOLOGY_BYTES;
    let mut values = vec![root];
    while let Some(value) = values.pop() {
        let (cost, children): (usize, &[Value]) = match value {
            Value::Nil | Value::Okay => (0, &[]),
            Value::Int(_) | Value::Double(_) | Value::Boolean(_) => (8, &[]),
            Value::BulkString(value) => (value.len(), &[]),
            Value::SimpleString(value) => (value.len(), &[]),
            Value::VerbatimString { text, .. } => (text.len(), &[]),
            Value::BigNumber(value) => (value.to_string().len(), &[]),
            Value::Array(values) | Value::Set(values) | Value::Push { data: values, .. } => {
                (values.len().saturating_mul(8), values)
            }
            Value::Map(entries) => {
                let cost = entries.len().saturating_mul(16);
                if cost > remaining || entries.len() > remaining / 16 {
                    return Err(AppError::ClusterTopologyFailed);
                }
                for (key, value) in entries {
                    values.push(key);
                    values.push(value);
                }
                remaining -= cost;
                continue;
            }
            Value::Attribute { data, attributes } => {
                let cost = attributes.len().saturating_mul(16);
                if cost > remaining || attributes.len() > remaining / 16 {
                    return Err(AppError::ClusterTopologyFailed);
                }
                values.push(data);
                for (key, value) in attributes {
                    values.push(key);
                    values.push(value);
                }
                remaining -= cost;
                continue;
            }
            Value::ServerError(value) => (value.to_string().len(), &[]),
            _ => (0, &[]),
        };
        if cost > remaining || children.len() > remaining / 8 {
            return Err(AppError::ClusterTopologyFailed);
        }
        remaining -= cost;
        values.extend(children.iter());
    }
    Ok(())
}

fn bounded_text(value: &str) -> Result<&str, AppError> {
    if value.is_empty() || value.len() > MAX_ID_OR_HOST_BYTES {
        Err(AppError::ClusterTopologyFailed)
    } else {
        Ok(value)
    }
}

fn parse_u16(value: &str) -> Result<u16, AppError> {
    value
        .trim()
        .parse()
        .map_err(|_| AppError::ClusterTopologyFailed)
}

fn parse_u64(value: &str) -> Result<u64, AppError> {
    value
        .trim()
        .parse()
        .map_err(|_| AppError::ClusterTopologyFailed)
}

fn parse_f64(value: &str) -> Result<f64, AppError> {
    let value: f64 = value
        .trim()
        .parse()
        .map_err(|_| AppError::ClusterTopologyFailed)?;
    if value.is_finite() {
        Ok(value)
    } else {
        Err(AppError::ClusterTopologyFailed)
    }
}
