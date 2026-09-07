import { useEffect, useRef, useState } from "react";
import { getClusterTopology, refreshClusterTopology } from "../../lib/tauri";
import type { ClusterTopology } from "../../lib/types";
import { formatEndpoint } from "../connections/connectionState";
import { nodeHealthLabels, topologyMetricLabels } from "./topologyState";

export default function TopologyPage({
  connectionId,
}: {
  connectionId: string;
}) {
  return <TopologySession key={connectionId} connectionId={connectionId} />;
}

function TopologySession({ connectionId }: { connectionId: string }) {
  const [topology, setTopology] = useState<ClusterTopology | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState(false);
  const request = useRef(0);
  const active = useRef(false);
  const load = async (refresh: boolean) => {
    const token = ++request.current;
    setLoading(true);
    setError(false);
    try {
      const result = await (
        refresh ? refreshClusterTopology : getClusterTopology
      )(connectionId);
      if (active.current && request.current === token) setTopology(result);
    } catch {
      if (active.current && request.current === token) setError(true);
    } finally {
      if (active.current && request.current === token) setLoading(false);
    }
  };
  useEffect(() => {
    active.current = true;
    void load(false);
    return () => {
      active.current = false;
      ++request.current;
    };
    // Each mounted session is scoped to a connection ID.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  return (
    <section
      className="topology-page"
      aria-labelledby="topology-title"
      aria-busy={loading}
    >
      <div className="page-heading">
        <div>
          <p className="eyebrow">CLUSTER TOPOLOGY</p>
          <h2 id="topology-title">Cluster 拓扑</h2>
          <p className="page-description">
            查看全部节点、槽位与节点指标。不可用节点不影响其他节点结果。
          </p>
        </div>
        <button
          className="button button-secondary"
          type="button"
          disabled={loading}
          onClick={() => void load(true)}
        >
          {loading ? "加载拓扑中…" : "刷新拓扑"}
        </button>
      </div>
      {error && (
        <p role="alert" className="inline-error">
          无法加载 Cluster 拓扑，请刷新重试。
          {topology ? "当前显示上次结果。" : ""}
        </p>
      )}
      {topology && (
        <>
          {topology.failures.length > 0 && (
            <p role="alert" className="inline-error">
              {topology.failures.length}{" "}
              个节点暂不可用，当前为部分结果；刷新可重试。
            </p>
          )}
          <dl className="database-metric-grid">
            {[
              ["集群状态", topology.summary.state],
              ["已分配槽位", topology.summary.slots_assigned],
              ["正常槽位", topology.summary.slots_ok],
              ["疑似失败槽位", topology.summary.slots_pfail],
              ["失败槽位", topology.summary.slots_fail],
              [
                "主节点",
                topology.nodes.filter((n) => n.role === "primary").length,
              ],
              [
                "副本节点",
                topology.nodes.filter((n) => n.role === "replica").length,
              ],
              ["已知节点", topology.summary.known_nodes],
              ["配置纪元", topology.summary.current_epoch],
            ].map(([label, value]) => (
              <div className="database-metric-card" key={label}>
                <dt>{label}</dt>
                <dd>{value}</dd>
              </div>
            ))}
          </dl>
          {topology.nodes.length === 0 ? (
            <p className="empty-state-compact">暂无拓扑节点</p>
          ) : (
            <div
              className="topology-table-wrap"
              tabIndex={0}
              role="region"
              aria-label="Cluster 节点表，可横向滚动"
            >
              <table className="database-table topology-table">
                <thead>
                  <tr>
                    <th>节点</th>
                    <th>地址</th>
                    <th>角色</th>
                    <th>健康</th>
                    <th>主节点 ID</th>
                    <th>槽位</th>
                    {topologyMetricLabels.map(([key, label]) => (
                      <th key={key}>{label}</th>
                    ))}
                  </tr>
                </thead>
                <tbody>
                  {topology.nodes.map((node) => (
                    <tr key={node.id}>
                      <th scope="row">{node.id}</th>
                      <td>
                        {formatEndpoint(node.endpoint)}
                        {node.connection_endpoint && (
                          <small className="topology-dial-address">
                            连接：{formatEndpoint(node.connection_endpoint)}
                          </small>
                        )}
                      </td>
                      <td>{node.role === "primary" ? "主节点" : "副本"}</td>
                      <td>
                        {nodeHealthLabels[node.health]}
                        {topology.failures.some(
                          (failure) => failure.node_id === node.id,
                        ) && " · 指标不可用"}
                      </td>
                      <td>{node.primary_id ?? "—"}</td>
                      <td>
                        {node.slots
                          .map((slot) => `${slot.start}–${slot.end}`)
                          .join("、") || "—"}
                      </td>
                      {topologyMetricLabels.map(([key]) => (
                        <td key={key}>{node.metrics[key] ?? "不可用"}</td>
                      ))}
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </>
      )}
    </section>
  );
}
