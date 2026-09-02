use ::redis::Value;

use crate::{
    domain::{
        ClusterNode, ClusterNodeHealth, ClusterNodeMetrics, ClusterNodeRole, ClusterSummary,
        ConnectionEndpoint, SlotRange,
    },
    error::AppError,
};

const MAX_TOPOLOGY_TEXT_BYTES: usize = 4 * 1024 * 1024;
const MAX_DECODED_STRUCTURAL_ENVELOPE_BYTES: usize = 4 * 1024 * 1024;
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
            "cluster_slots_assigned" => summary.slots_assigned = parse_slot_count(value)?,
            "cluster_slots_ok" => summary.slots_ok = parse_slot_count(value)?,
            "cluster_slots_pfail" => summary.slots_pfail = parse_slot_count(value)?,
            "cluster_slots_fail" => summary.slots_fail = parse_slot_count(value)?,
            "cluster_current_epoch" => summary.current_epoch = parse_u64(value)?,
            "cluster_size" => summary.size = parse_node_count(value)?,
            "cluster_known_nodes" => summary.known_nodes = parse_node_count(value)?,
            _ => {}
        }
    }
    Ok(summary)
}

pub fn parse_cluster_shards(value: Value) -> Result<Vec<ClusterNode>, AppError> {
    parse_cluster_shards_for_tls(value, false)
}

/// Parses a decoded `CLUSTER SHARDS` reply using the endpoint port appropriate for `tls`.
/// This is display topology only; `connection_endpoint` remains unset until a live service
/// has successfully connected to the node.
pub fn parse_cluster_shards_for_tls(value: Value, tls: bool) -> Result<Vec<ClusterNode>, AppError> {
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
        parse_shard(shard, &mut nodes, &mut slot_range_count, tls)?;
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
        slot_range_count = slot_range_count
            .checked_add(node.slots.len())
            .ok_or(AppError::ClusterTopologyFailed)?;
        nodes.push(node);
    }
    Ok(nodes)
}

pub fn merge_node_metrics(node: &mut ClusterNode, input: &str) -> Result<(), AppError> {
    ensure_text_size(input)?;
    let mut metrics = node.metrics.clone();
    let mut hits = None;
    let mut misses = None;
    for line in input.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        match key {
            "used_memory" => set_metric(&mut metrics.used_memory_bytes, parse_u64(value)),
            "instantaneous_ops_per_sec" => {
                set_metric(&mut metrics.ops_per_second, parse_u64(value))
            }
            "total_connections_received" => {
                set_metric(&mut metrics.connections_received, parse_u64(value))
            }
            "connected_clients" => set_metric(&mut metrics.connected_clients, parse_u64(value)),
            "total_commands_processed" => {
                set_metric(&mut metrics.commands_processed, parse_u64(value))
            }
            "instantaneous_input_kbps" => {
                set_metric(&mut metrics.network_in_kbps, parse_f64(value))
            }
            "instantaneous_output_kbps" => {
                set_metric(&mut metrics.network_out_kbps, parse_f64(value))
            }
            "master_repl_offset" | "slave_repl_offset" => {
                set_metric(&mut metrics.replication_offset, parse_u64(value))
            }
            "master_last_io_seconds_ago" => {
                set_metric(&mut metrics.replication_lag, parse_u64(value))
            }
            "uptime_in_seconds" => set_metric(&mut metrics.uptime_seconds, parse_u64(value)),
            "keyspace_hits" => hits = parse_u64(value).ok(),
            "keyspace_misses" => misses = parse_u64(value).ok(),
            _ => {}
        }
    }
    if let (Some(hits), Some(misses)) = (hits, misses) {
        if let Some(total) = hits.checked_add(misses).filter(|total| *total != 0) {
            metrics.cache_hit_ratio = Some(hits as f64 * 100.0 / total as f64);
        }
    }
    node.metrics = metrics;
    Ok(())
}

fn set_metric<T>(target: &mut Option<T>, result: Result<T, AppError>) {
    if let Ok(value) = result {
        *target = Some(value);
    }
}

fn parse_shard(
    shard: &Value,
    output: &mut Vec<ClusterNode>,
    slot_range_count: &mut usize,
    tls: bool,
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
    *slot_range_count = slot_range_count
        .checked_add(slots.len())
        .ok_or(AppError::ClusterTopologyFailed)?;
    let shard_nodes = sequence(value_without_attributes(
        shard_nodes.ok_or(AppError::ClusterTopologyFailed)?,
    ))
    .ok_or(AppError::ClusterTopologyFailed)?;
    let remaining_nodes = MAX_NODES
        .checked_sub(output.len())
        .ok_or(AppError::ClusterTopologyFailed)?;
    if shard_nodes.len() > remaining_nodes {
        return Err(AppError::ClusterTopologyFailed);
    }
    let mut parsed = Vec::with_capacity(shard_nodes.len());
    for value in shard_nodes {
        parsed.push(parse_shard_node(value, tls)?);
    }
    let mut primary = None;
    for (index, node) in parsed.iter().enumerate() {
        if node.role == ClusterNodeRole::Primary {
            if primary.replace(index).is_some() {
                return Err(AppError::ClusterTopologyFailed);
            }
        }
    }
    let primary = primary.ok_or(AppError::ClusterTopologyFailed)?;
    let primary_id = parsed[primary].id.clone();
    for (index, node) in parsed.iter_mut().enumerate() {
        if index == primary {
            node.slots = slots.clone();
        } else {
            node.primary_id = Some(primary_id.clone());
        }
    }
    output.extend(parsed);
    Ok(())
}

fn parse_shard_node(value: &Value, tls: bool) -> Result<ClusterNode, AppError> {
    let mut id = None;
    let mut endpoint = None;
    let mut hostname = None;
    let mut ip = None;
    let mut port = None;
    let mut tls_port = None;
    let mut role = None;
    let mut health = None;

    for_each_pair(value, |key, value| {
        let Some(key) = value_as_text(key) else {
            return;
        };
        match key {
            "id" => id = value_as_text(value),
            "endpoint" => endpoint = value_as_text(value),
            "hostname" => hostname = value_as_text(value),
            "ip" => ip = value_as_text(value),
            "port" => port = value_as_u16(value),
            "tls-port" => tls_port = value_as_u16(value),
            "role" => role = value_as_text(value),
            "health" => health = value_as_text(value),
            _ => {}
        }
    })?;

    let id = bounded_text(id.ok_or(AppError::ClusterTopologyFailed)?)?.to_owned();
    let host = endpoint
        .filter(|value| !value.is_empty())
        .or(hostname.filter(|value| !value.is_empty()))
        .or(ip.filter(|value| !value.is_empty()))
        .ok_or(AppError::ClusterTopologyFailed)?;
    let host = bounded_text(host)?.to_owned();
    let port = if tls {
        tls_port
            .filter(|port| *port != 0)
            .or(port.filter(|port| *port != 0))
    } else {
        port.filter(|port| *port != 0)
            .or(tls_port.filter(|port| *port != 0))
    };
    let endpoint = ConnectionEndpoint {
        host,
        port: port.ok_or(AppError::ClusterTopologyFailed)?,
    };
    let role = parse_role(role.ok_or(AppError::ClusterTopologyFailed)?)?;
    let health = parse_health(health.unwrap_or("offline"));
    Ok(ClusterNode {
        id,
        endpoint,
        connection_endpoint: None,
        role,
        health,
        primary_id: None,
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
    fields.next().ok_or(AppError::ClusterTopologyFailed)?;
    fields.next().ok_or(AppError::ClusterTopologyFailed)?;
    fields.next().ok_or(AppError::ClusterTopologyFailed)?;
    let link_state = fields.next().ok_or(AppError::ClusterTopologyFailed)?;

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
    let health = if link_state != "connected"
        || flags
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
    let (address, announced_hostname) = value.split_once(',').unwrap_or((value, ""));
    let address = address
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
        host: bounded_text(if announced_hostname.is_empty() {
            host
        } else {
            announced_hostname
        })?
        .to_owned(),
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
    if value.len() > MAX_TOPOLOGY_TEXT_BYTES {
        Err(AppError::ClusterTopologyFailed)
    } else {
        Ok(())
    }
}

fn ensure_value_size(root: &Value) -> Result<(), AppError> {
    // redis-rs has already normalized the wire reply. This checked decoded structural-envelope
    // limit bounds payload and decoded RESP structure only; it cannot recover raw wire details
    // such as leading zeros, so transport owns any exact raw-byte cap before decoding.
    let mut total = 0usize;
    let mut stack = vec![root];
    while let Some(value) = stack.pop() {
        match value {
            Value::Nil => add_envelope_bytes(&mut total, 5)?,
            Value::Okay => add_envelope_bytes(&mut total, 5)?,
            Value::Int(_) => add_envelope_bytes(&mut total, 23)?,
            Value::Double(_) => add_envelope_bytes(&mut total, 29)?,
            Value::Boolean(_) => add_envelope_bytes(&mut total, 4)?,
            Value::BulkString(value) => {
                add_envelope_bytes(&mut total, bulk_envelope_size(value.len())?)?
            }
            Value::SimpleString(value) => {
                add_envelope_bytes(&mut total, simple_envelope_size(value.len())?)?
            }
            Value::VerbatimString { text, .. } => add_envelope_bytes(
                &mut total,
                bulk_envelope_size(
                    text.len()
                        .checked_add(4)
                        .ok_or(AppError::ClusterTopologyFailed)?,
                )?,
            )?,
            Value::BigNumber(value) => {
                add_envelope_bytes(&mut total, simple_envelope_size(value.to_string().len())?)?
            }
            Value::Array(values) | Value::Set(values) => {
                add_envelope_bytes(&mut total, aggregate_envelope_size(values.len())?)?;
                push_values(&mut stack, values)?;
            }
            Value::Push { kind, data } => {
                let item_count = data
                    .len()
                    .checked_add(1)
                    .ok_or(AppError::ClusterTopologyFailed)?;
                add_envelope_bytes(&mut total, aggregate_envelope_size(item_count)?)?;
                // redis-rs stores the push kind outside `data`; account for it explicitly,
                // including a future/unknown `PushKind::Other` string.
                add_envelope_bytes(&mut total, simple_envelope_size(kind.to_string().len())?)?;
                push_values(&mut stack, data)?;
            }
            Value::Map(entries) => {
                add_envelope_bytes(&mut total, aggregate_envelope_size(entries.len())?)?;
                reserve_value_stack(
                    &stack,
                    entries
                        .len()
                        .checked_mul(2)
                        .ok_or(AppError::ClusterTopologyFailed)?,
                )?;
                for (key, value) in entries {
                    stack.push(key);
                    stack.push(value);
                }
            }
            Value::Attribute { data, attributes } => {
                add_envelope_bytes(&mut total, aggregate_envelope_size(attributes.len())?)?;
                reserve_value_stack(
                    &stack,
                    attributes
                        .len()
                        .checked_mul(2)
                        .and_then(|length| length.checked_add(1))
                        .ok_or(AppError::ClusterTopologyFailed)?,
                )?;
                stack.push(data);
                for (key, value) in attributes {
                    stack.push(key);
                    stack.push(value);
                }
            }
            Value::ServerError(value) => {
                let detail = value.details().map(str::len).unwrap_or(0);
                let body = value
                    .code()
                    .len()
                    .checked_add(detail)
                    .and_then(|length| length.checked_add(1))
                    .ok_or(AppError::ClusterTopologyFailed)?;
                add_envelope_bytes(&mut total, simple_envelope_size(body)?)?;
            }
            _ => {}
        }
    }
    Ok(())
}

fn add_envelope_bytes(total: &mut usize, amount: usize) -> Result<(), AppError> {
    *total = total
        .checked_add(amount)
        .filter(|total| *total <= MAX_DECODED_STRUCTURAL_ENVELOPE_BYTES)
        .ok_or(AppError::ClusterTopologyFailed)?;
    Ok(())
}

fn bulk_envelope_size(length: usize) -> Result<usize, AppError> {
    1usize
        .checked_add(decimal_digits(length))
        .and_then(|size| size.checked_add(2))
        .and_then(|size| size.checked_add(length))
        .and_then(|size| size.checked_add(2))
        .ok_or(AppError::ClusterTopologyFailed)
}

fn simple_envelope_size(length: usize) -> Result<usize, AppError> {
    length.checked_add(3).ok_or(AppError::ClusterTopologyFailed)
}

fn aggregate_envelope_size(length: usize) -> Result<usize, AppError> {
    decimal_digits(length)
        .checked_add(3)
        .ok_or(AppError::ClusterTopologyFailed)
}

fn decimal_digits(mut value: usize) -> usize {
    let mut digits = 1;
    while value >= 10 {
        value /= 10;
        digits += 1;
    }
    digits
}

fn push_values<'a>(stack: &mut Vec<&'a Value>, children: &'a [Value]) -> Result<(), AppError> {
    reserve_value_stack(stack, children.len())?;
    stack.extend(children.iter());
    Ok(())
}

fn reserve_value_stack(stack: &Vec<&Value>, additional: usize) -> Result<(), AppError> {
    stack
        .len()
        .checked_add(additional)
        .filter(|length| *length <= MAX_DECODED_STRUCTURAL_ENVELOPE_BYTES)
        .ok_or(AppError::ClusterTopologyFailed)?;
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

fn parse_slot_count(value: &str) -> Result<u16, AppError> {
    let value = parse_u16(value)?;
    if value <= 16_384 {
        Ok(value)
    } else {
        Err(AppError::ClusterTopologyFailed)
    }
}

fn parse_node_count(value: &str) -> Result<u16, AppError> {
    let value = parse_u16(value)?;
    if value as usize <= MAX_NODES {
        Ok(value)
    } else {
        Err(AppError::ClusterTopologyFailed)
    }
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
