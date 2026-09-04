use std::collections::{HashMap, HashSet};

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use futures_util::{future::BoxFuture, stream, StreamExt};
use serde::{Deserialize, Serialize};

use crate::{
    domain::{ConnectionEndpoint, NodeFailure},
    error::AppError,
};

const CURSOR_PREFIX: &str = "cluster:";
const CURSOR_VERSION: u8 = 1;
const MAX_CURSOR_BYTES: usize = 32 * 1024;
const MAX_NODES: usize = 128;
const MAX_PAGE_BYTES: usize = 4 * 1024 * 1024;
const MAX_CONCURRENCY: usize = 8;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct NodeScanCursor {
    pub node_id: String,
    pub cursor: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ClusterScanState {
    pub generation: u64,
    pub nodes: Vec<NodeScanCursor>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct VersionedClusterScanState {
    version: u8,
    generation: u64,
    nodes: Vec<NodeScanCursor>,
    pending_nodes: Vec<String>,
    next_node: usize,
}

impl ClusterScanState {
    pub fn new(generation: u64, nodes: Vec<NodeScanCursor>) -> Self {
        Self { generation, nodes }
    }

    pub fn cursor_for(&self, node_id: &str) -> Option<u64> {
        self.nodes
            .iter()
            .find(|node| node.node_id == node_id)
            .map(|node| node.cursor)
    }

    pub fn encode(&self) -> Result<String, AppError> {
        validate_node_cursors(&self.nodes)?;
        let wire = VersionedClusterScanState {
            version: CURSOR_VERSION,
            generation: self.generation,
            pending_nodes: self.nodes.iter().map(|node| node.node_id.clone()).collect(),
            nodes: self.nodes.clone(),
            next_node: 0,
        };
        encode_wire(&wire)
    }

    pub fn decode(
        cursor: &str,
        generation: u64,
        known_nodes: &HashSet<String>,
    ) -> Result<Self, AppError> {
        let wire = decode_wire(cursor, generation, known_nodes)?;
        Ok(Self {
            generation: wire.generation,
            nodes: wire.nodes,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClusterScanNode {
    pub node_id: String,
    pub endpoint: ConnectionEndpoint,
}

pub trait ClusterScanBackend: Send + Sync {
    fn scan_node<'a>(
        &'a self,
        node: &'a ClusterScanNode,
        cursor: u64,
        pattern: &'a str,
        count: usize,
    ) -> BoxFuture<'a, Result<(u64, Vec<Vec<u8>>), AppError>>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClusterScanPage {
    pub cursor: String,
    pub keys: Vec<Vec<u8>>,
    pub node_failures: Vec<NodeFailure>,
    pub has_more: bool,
}

pub async fn scan_cluster<B: ClusterScanBackend>(
    backend: &B,
    generation: u64,
    nodes: &[ClusterScanNode],
    cursor: Option<&str>,
    pattern: &str,
    count: usize,
) -> Result<ClusterScanPage, AppError> {
    validate_scan_request(nodes, pattern, count)?;
    let known_nodes = nodes
        .iter()
        .map(|node| node.node_id.clone())
        .collect::<HashSet<_>>();
    let mut wire = match cursor {
        Some(cursor) => decode_wire(cursor, generation, &known_nodes)?,
        None => VersionedClusterScanState {
            version: CURSOR_VERSION,
            generation,
            nodes: nodes
                .iter()
                .map(|node| NodeScanCursor {
                    node_id: node.node_id.clone(),
                    cursor: 0,
                })
                .collect(),
            pending_nodes: nodes.iter().map(|node| node.node_id.clone()).collect(),
            next_node: 0,
        },
    };

    if wire.pending_nodes.is_empty() {
        return Ok(ClusterScanPage {
            cursor: encode_wire(&wire)?,
            keys: Vec::new(),
            node_failures: Vec::new(),
            has_more: false,
        });
    }

    let node_by_id = nodes
        .iter()
        .map(|node| (node.node_id.as_str(), node))
        .collect::<HashMap<_, _>>();
    let cursor_by_id = wire
        .nodes
        .iter()
        .map(|node| (node.node_id.as_str(), node.cursor))
        .collect::<HashMap<_, _>>();
    let ordered_pending = round_robin_pending(&wire, nodes);
    let selected = ordered_pending
        .into_iter()
        .take(MAX_CONCURRENCY)
        .collect::<Vec<_>>();
    let per_node_count = count
        .checked_add(wire.pending_nodes.len() - 1)
        .ok_or(AppError::InvalidInput)?
        / wire.pending_nodes.len();

    let mut results = stream::iter(selected.clone().into_iter().enumerate().map(
        |(position, node_id)| {
            let node = node_by_id[node_id.as_str()].clone();
            let node_cursor = cursor_by_id[node_id.as_str()];
            async move {
                (
                    position,
                    node_id.clone(),
                    backend
                        .scan_node(&node, node_cursor, pattern, per_node_count.max(1))
                        .await,
                )
            }
        },
    ))
    .buffer_unordered(MAX_CONCURRENCY)
    .collect::<Vec<_>>()
    .await;
    results.sort_by_key(|(position, _, _)| *position);

    let mut keys = Vec::new();
    let mut seen = HashSet::new();
    let mut page_bytes = 0_usize;
    let mut failures = Vec::new();
    let mut pending = wire.pending_nodes.iter().cloned().collect::<HashSet<_>>();
    let mut next_cursors = wire
        .nodes
        .iter()
        .map(|node| (node.node_id.clone(), node.cursor))
        .collect::<HashMap<_, _>>();
    for (_, node_id, result) in results {
        match result {
            Ok((next_cursor, batch)) => {
                let batch_bytes = batch.iter().try_fold(0_usize, |total, key| {
                    total.checked_add(key.len()).ok_or(AppError::InvalidInput)
                })?;
                page_bytes = page_bytes
                    .checked_add(batch_bytes)
                    .filter(|bytes| *bytes <= MAX_PAGE_BYTES)
                    .ok_or(AppError::InvalidInput)?;
                for key in batch {
                    if seen.insert(key.clone()) {
                        keys.push(key);
                    }
                }
                next_cursors.insert(node_id.clone(), next_cursor);
                if next_cursor == 0 {
                    pending.remove(&node_id);
                }
            }
            Err(_) => failures.push(NodeFailure {
                node_id,
                code: AppError::ClusterNodeUnavailable.code().to_owned(),
            }),
        }
    }

    for node in &mut wire.nodes {
        node.cursor = next_cursors[&node.node_id];
    }
    wire.pending_nodes = wire
        .nodes
        .iter()
        .filter(|node| pending.contains(&node.node_id))
        .map(|node| node.node_id.clone())
        .collect();
    wire.next_node = if nodes.is_empty() {
        0
    } else {
        (wire.next_node + selected.len()) % nodes.len()
    };
    let has_more = !wire.pending_nodes.is_empty();
    Ok(ClusterScanPage {
        cursor: encode_wire(&wire)?,
        keys,
        node_failures: failures,
        has_more,
    })
}

fn round_robin_pending(wire: &VersionedClusterScanState, nodes: &[ClusterScanNode]) -> Vec<String> {
    let pending = wire.pending_nodes.iter().collect::<HashSet<_>>();
    (0..nodes.len())
        .map(|offset| &nodes[(wire.next_node + offset) % nodes.len()].node_id)
        .filter(|node_id| pending.contains(node_id))
        .cloned()
        .collect()
}

fn validate_scan_request(
    nodes: &[ClusterScanNode],
    pattern: &str,
    count: usize,
) -> Result<(), AppError> {
    if nodes.is_empty()
        || nodes.len() > MAX_NODES
        || pattern.is_empty()
        || count == 0
        || nodes.iter().any(|node| node.node_id.is_empty())
        || nodes
            .iter()
            .map(|node| &node.node_id)
            .collect::<HashSet<_>>()
            .len()
            != nodes.len()
    {
        return Err(AppError::InvalidInput);
    }
    Ok(())
}

fn validate_node_cursors(nodes: &[NodeScanCursor]) -> Result<(), AppError> {
    if nodes.len() > MAX_NODES
        || nodes.iter().any(|node| node.node_id.is_empty())
        || nodes
            .iter()
            .map(|node| &node.node_id)
            .collect::<HashSet<_>>()
            .len()
            != nodes.len()
    {
        return Err(AppError::InvalidInput);
    }
    Ok(())
}

fn encode_wire(wire: &VersionedClusterScanState) -> Result<String, AppError> {
    validate_node_cursors(&wire.nodes)?;
    let encoded = serde_json::to_vec(wire).map_err(|_| AppError::InvalidInput)?;
    let encoded = URL_SAFE_NO_PAD.encode(encoded);
    if encoded.len() > MAX_CURSOR_BYTES {
        return Err(AppError::InvalidInput);
    }
    Ok(format!("{CURSOR_PREFIX}{encoded}"))
}

fn decode_wire(
    cursor: &str,
    generation: u64,
    known_nodes: &HashSet<String>,
) -> Result<VersionedClusterScanState, AppError> {
    let encoded = cursor
        .strip_prefix(CURSOR_PREFIX)
        .ok_or(AppError::InvalidInput)?;
    if encoded.len() > MAX_CURSOR_BYTES {
        return Err(AppError::InvalidInput);
    }
    let decoded = URL_SAFE_NO_PAD
        .decode(encoded)
        .map_err(|_| AppError::InvalidInput)?;
    let wire = serde_json::from_slice::<VersionedClusterScanState>(&decoded)
        .map_err(|_| AppError::InvalidInput)?;
    validate_node_cursors(&wire.nodes)?;
    let node_ids = wire
        .nodes
        .iter()
        .map(|node| node.node_id.clone())
        .collect::<HashSet<_>>();
    let pending = wire.pending_nodes.iter().collect::<HashSet<_>>();
    if wire.version != CURSOR_VERSION
        || wire.generation != generation
        || node_ids != *known_nodes
        || wire.pending_nodes.len() != pending.len()
        || !wire
            .pending_nodes
            .iter()
            .all(|node| node_ids.contains(node))
        || (!wire.nodes.is_empty() && wire.next_node >= wire.nodes.len())
        || (wire.nodes.is_empty() && wire.next_node != 0)
    {
        return Err(AppError::InvalidInput);
    }
    Ok(wire)
}
