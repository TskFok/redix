import type { ClusterNodeMetrics } from "../../lib/types";

export const topologyMetricLabels: Array<[keyof ClusterNodeMetrics, string]> = [
  ["used_memory_bytes", "内存 (bytes)"],
  ["ops_per_second", "操作 / 秒"],
  ["connected_clients", "客户端"],
  ["connections_received", "累计连接"],
  ["commands_processed", "累计命令"],
  ["network_in_kbps", "网络入 (KB/s)"],
  ["network_out_kbps", "网络出 (KB/s)"],
  ["cache_hit_ratio", "缓存命中率"],
  ["replication_offset", "复制偏移"],
  ["replication_lag", "复制延迟"],
  ["uptime_seconds", "运行时间 (秒)"],
];
export const nodeHealthLabels = {
  online: "在线",
  offline: "离线",
  loading: "加载中",
};
