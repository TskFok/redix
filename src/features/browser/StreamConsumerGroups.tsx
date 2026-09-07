import { useEffect, useRef, useState } from "react";

import {
  acknowledgeStreamPendingEntries,
  claimStreamPendingEntries,
  createStreamConsumerGroup,
  deleteStreamConsumer,
  deleteStreamConsumerGroup,
  getStreamConsumerGroups,
  getStreamConsumers,
  getStreamPendingEntries,
} from "../../lib/tauri";
import type {
  StreamConsumer,
  StreamConsumerGroup,
  StreamPendingEntry,
} from "../../lib/types";
import { browserErrorMessage } from "./browserState";
import StreamAdvancedPanel from "./StreamAdvancedPanel";

interface StreamConsumerGroupsProps {
  connectionId: string;
  streamKey: string;
}

function formatIdle(idleMs: number): string {
  return `${idleMs} ms`;
}

export function StreamConsumerGroups({
  connectionId,
  streamKey,
}: StreamConsumerGroupsProps) {
  const [groups, setGroups] = useState<StreamConsumerGroup[]>([]);
  const [selectedGroupName, setSelectedGroupName] = useState("");
  const [consumers, setConsumers] = useState<StreamConsumer[]>([]);
  const [pending, setPending] = useState<StreamPendingEntry[]>([]);
  const [selectedPendingIds, setSelectedPendingIds] = useState<string[]>([]);
  const [claimConsumer, setClaimConsumer] = useState("");
  const [claimIdle, setClaimIdle] = useState("0");
  const [claimNotice, setClaimNotice] = useState<string | null>(null);
  const scopeRef = useRef("");
  scopeRef.current = JSON.stringify([connectionId, streamKey, selectedGroupName]);
  const mutationRef = useRef(0);
  const [groupName, setGroupName] = useState("");
  const [lastDeliveredId, setLastDeliveredId] = useState("$");
  const [loading, setLoading] = useState(false);
  const [detailsLoading, setDetailsLoading] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [detailsRefresh, setDetailsRefresh] = useState(0);
  const mountedRef = useRef(false);
  const groupsRequestRef = useRef(0);
  const detailsRequestRef = useRef(0);
  const connectionRef = useRef(connectionId);
  const streamKeyRef = useRef(streamKey);
  connectionRef.current = connectionId;
  streamKeyRef.current = streamKey;

  useEffect(() => {
    mutationRef.current += 1;
    setBusy(false);
    setClaimNotice(null);
  }, [connectionId, streamKey, selectedGroupName]);

  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
      groupsRequestRef.current += 1;
      detailsRequestRef.current += 1;
    };
  }, []);

  const isGroupCurrent = (token: number) =>
    mountedRef.current &&
    groupsRequestRef.current === token &&
    connectionRef.current === connectionId &&
    streamKeyRef.current === streamKey;

  const isDetailsCurrent = (token: number) =>
    mountedRef.current &&
    detailsRequestRef.current === token &&
    connectionRef.current === connectionId &&
    streamKeyRef.current === streamKey;

  const loadGroups = async () => {
    const token = groupsRequestRef.current + 1;
    groupsRequestRef.current = token;
    setLoading(true);
    setError(null);
    try {
      const nextGroups = await getStreamConsumerGroups({
        connection_id: connectionId,
        key: streamKey,
      });
      if (!isGroupCurrent(token)) {
        return;
      }
      setGroups(nextGroups);
      setSelectedGroupName((current) =>
        nextGroups.some((group) => group.name === current)
          ? current
          : nextGroups[0]?.name ?? "",
      );
    } catch (caught) {
      if (isGroupCurrent(token)) {
        setError(browserErrorMessage(caught, "加载 Consumer Groups 失败，请稍后重试。"));
        setGroups([]);
        setSelectedGroupName("");
      }
    } finally {
      if (isGroupCurrent(token)) {
        setLoading(false);
      }
    }
  };

  useEffect(() => {
    groupsRequestRef.current += 1;
    detailsRequestRef.current += 1;
    setBusy(false);
    setClaimConsumer("");
    setClaimIdle("0");
    setGroups([]);
    setSelectedGroupName("");
    setConsumers([]);
    setPending([]);
    setSelectedPendingIds([]);
    setError(null);
    void loadGroups();
  }, [connectionId, streamKey]);

  useEffect(() => {
    const token = detailsRequestRef.current + 1;
    detailsRequestRef.current = token;
    setSelectedPendingIds([]);
    setClaimNotice(null);
    if (!selectedGroupName) {
      setConsumers([]);
      setPending([]);
      setDetailsLoading(false);
      return;
    }

    setDetailsLoading(true);
    setError(null);
    const groupInput = {
      connection_id: connectionId,
      key: streamKey,
      group: selectedGroupName,
    };
    Promise.all([
      getStreamConsumers(groupInput),
      getStreamPendingEntries({ ...groupInput, count: 100, consumer: null }),
    ])
      .then(([nextConsumers, nextPending]) => {
        if (!isDetailsCurrent(token)) {
          return;
        }
        setConsumers(nextConsumers);
        setPending(nextPending);
      })
      .catch((caught) => {
        if (isDetailsCurrent(token)) {
          setError(browserErrorMessage(caught, "加载 Consumer Group 详情失败，请稍后重试。"));
          setConsumers([]);
          setPending([]);
        }
      })
      .finally(() => {
        if (isDetailsCurrent(token)) {
          setDetailsLoading(false);
        }
      });
  }, [connectionId, streamKey, selectedGroupName, detailsRefresh]);

  const selectedGroup = groups.find((group) => group.name === selectedGroupName) ?? null;

  const refresh = async () => {
    await loadGroups();
    if (mountedRef.current) {
      setDetailsRefresh((current) => current + 1);
    }
  };

  const handleCreateGroup = async (event: React.FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const trimmedName = groupName.trim();
    const trimmedId = lastDeliveredId.trim();
    if (trimmedName === "") {
      setError("Consumer Group 名称不能为空。");
      return;
    }
    if (trimmedId === "") {
      setError("起始 ID 不能为空。");
      return;
    }

    setBusy(true);
    setError(null);
    try {
      await createStreamConsumerGroup({
        connection_id: connectionId,
        key: streamKey,
        name: trimmedName,
        last_delivered_id: trimmedId,
      });
      setGroupName("");
      await refresh();
    } catch (caught) {
      setError(browserErrorMessage(caught, "创建 Consumer Group 失败，请稍后重试。"));
    } finally {
      setBusy(false);
    }
  };

  const handleDeleteGroup = async () => {
    if (!selectedGroup || !window.confirm(`确定删除 Consumer Group “${selectedGroup.name}”吗？`)) {
      return;
    }
    setBusy(true);
    setError(null);
    try {
      await deleteStreamConsumerGroup({
        connection_id: connectionId,
        key: streamKey,
        name: selectedGroup.name,
      });
      await refresh();
    } catch (caught) {
      setError(browserErrorMessage(caught, "删除 Consumer Group 失败，请稍后重试。"));
    } finally {
      setBusy(false);
    }
  };

  const handleDeleteConsumer = async (consumer: StreamConsumer) => {
    if (
      !selectedGroup ||
      !window.confirm(`确定删除消费者“${consumer.name}”吗？这会移除其 Pending 记录。`)
    ) {
      return;
    }
    setBusy(true);
    setError(null);
    try {
      await deleteStreamConsumer({
        connection_id: connectionId,
        key: streamKey,
        group: selectedGroup.name,
        consumer: consumer.name,
      });
      await refresh();
    } catch (caught) {
      setError(browserErrorMessage(caught, "删除消费者失败，请稍后重试。"));
    } finally {
      setBusy(false);
    }
  };

  const handleAcknowledge = async () => {
    if (!selectedGroup || selectedPendingIds.length === 0) {
      return;
    }
    setBusy(true);
    setError(null);
    try {
      await acknowledgeStreamPendingEntries({
        connection_id: connectionId,
        key: streamKey,
        group: selectedGroup.name,
        entries: selectedPendingIds,
      });
      setSelectedPendingIds([]);
      await refresh();
    } catch (caught) {
      setError(browserErrorMessage(caught, "确认 Pending 消息失败，请稍后重试。"));
    } finally {
      setBusy(false);
    }
  };

  const handleClaim = async () => {
    const minIdleMs = Number(claimIdle);
    if (!selectedGroup || selectedPendingIds.length === 0) return;
    if (!claimConsumer.trim() || claimConsumer.length > 256 || !/^\d+$/.test(claimIdle)
        || !Number.isSafeInteger(minIdleMs) || minIdleMs < 0) {
      setError("请填写目标消费者和有效的非负整数空闲时间。");
      return;
    }
    if (!window.confirm(`将 ${selectedPendingIds.length} 条 Pending 消息转移给“${claimConsumer}”？这会改变消息所属消费者。`)) return;
    const scope = scopeRef.current;
    const token = ++mutationRef.current;
    const isCurrent = () => mountedRef.current && scopeRef.current === scope && mutationRef.current === token;
    const entries = [...selectedPendingIds];
    setBusy(true); setError(null); setClaimNotice(null);
    try {
      const claimed = await claimStreamPendingEntries({
        connection_id: connectionId, key: streamKey, group: selectedGroup.name,
        consumer: claimConsumer, min_idle_ms: minIdleMs, entries,
      });
      if (!isCurrent()) return;
      setSelectedPendingIds([]);
      await refresh();
      if (isCurrent()) setClaimNotice(`已转移 ${claimed.length} / ${entries.length} 条；不满足空闲条件或已不在 Pending 的消息不会转移。`);
    } catch (caught) {
      if (isCurrent()) setError(browserErrorMessage(caught, "转移 Pending 消息失败，请稍后重试。"));
    } finally {
      if (isCurrent()) setBusy(false);
    }
  };

  const togglePending = (id: string) => {
    setSelectedPendingIds((current) =>
      current.includes(id) ? current.filter((entryId) => entryId !== id) : [...current, id],
    );
  };

  const allPendingSelected =
    pending.length > 0 && pending.every((entry) => selectedPendingIds.includes(entry.id));

  return (
    <section className="stream-consumer-groups" aria-labelledby="stream-consumer-groups-title">
      <div className="stream-groups-heading">
        <div>
          <p className="eyebrow">STREAM OPERATIONS</p>
          <h3 id="stream-consumer-groups-title">Consumer Groups</h3>
        </div>
        <button
          type="button"
          className="button button-quiet button-compact"
          onClick={() => void refresh()}
          disabled={busy || loading}
        >
          刷新 Groups
        </button>
      </div>

      <form className="stream-group-create" onSubmit={(event) => void handleCreateGroup(event)}>
        <label className="field">
          <span>Consumer Group 名称</span>
          <input
            aria-label="Consumer Group 名称"
            value={groupName}
            onChange={(event) => setGroupName(event.target.value)}
            disabled={busy}
            maxLength={256}
            spellCheck={false}
          />
        </label>
        <label className="field">
          <span>起始 ID</span>
          <input
            aria-label="起始 ID"
            value={lastDeliveredId}
            onChange={(event) => setLastDeliveredId(event.target.value)}
            disabled={busy}
            maxLength={256}
            spellCheck={false}
          />
        </label>
        <button type="submit" className="button button-primary" disabled={busy}>
          创建 Group
        </button>
      </form>

      {error ? (
        <p className="form-error" role="alert">
          {error}
        </p>
      ) : null}

      <div className="stream-group-selector">
        <label className="field">
          <span>选择 Consumer Group</span>
          <select
            aria-label="选择 Consumer Group"
            value={selectedGroupName}
            onChange={(event) => setSelectedGroupName(event.target.value)}
            disabled={busy || groups.length === 0}
          >
            <option value="">{loading ? "正在加载…" : "请选择 Consumer Group"}</option>
            {groups.map((group) => (
              <option key={group.name} value={group.name}>
                {group.name}
              </option>
            ))}
          </select>
        </label>
        <button
          type="button"
          className="button button-danger"
          onClick={() => void handleDeleteGroup()}
          disabled={busy || selectedGroup === null}
        >
          删除 Group
        </button>
      </div>

      {selectedGroup ? (
        <div className="stream-group-summary" aria-label="Consumer Group 摘要">
          <div>
            <span>Pending</span>
            <strong>{selectedGroup.pending}</strong>
          </div>
          <div>
            <span>消费者</span>
            <strong>{selectedGroup.consumers}</strong>
          </div>
          <div>
            <span>最后投递 ID</span>
            <code>{selectedGroup.last_delivered_id}</code>
          </div>
        </div>
      ) : null}

      {selectedGroup && <StreamAdvancedPanel
        key={JSON.stringify([connectionId, streamKey, selectedGroup.name])}
        connectionId={connectionId}
        streamKey={streamKey}
        group={selectedGroup.name}
        lastDeliveredId={selectedGroup.last_delivered_id}
        disabled={busy || loading || detailsLoading}
        onChanged={refresh}
        onBusyChange={setBusy}
      />}

      <div className="stream-group-data-grid" aria-busy={detailsLoading}>
        <section className="stream-group-subpanel" aria-labelledby="stream-consumers-title">
          <div className="stream-subpanel-heading">
            <h4 id="stream-consumers-title">消费者</h4>
            <span>{consumers.length}</span>
          </div>
          {consumers.length === 0 ? (
            <p className="stream-empty-state">
              {detailsLoading ? "正在加载消费者…" : "当前 Group 没有消费者。"}
            </p>
          ) : (
            <div className="stream-table-wrap">
              <table className="stream-groups-table">
                <thead>
                  <tr>
                    <th>名称</th>
                    <th>Pending</th>
                    <th>空闲</th>
                    <th aria-label="操作" />
                  </tr>
                </thead>
                <tbody>
                  {consumers.map((consumer) => (
                    <tr key={consumer.name}>
                      <td><code>{consumer.name}</code></td>
                      <td>{consumer.pending}</td>
                      <td>{formatIdle(consumer.idle_ms)}</td>
                      <td>
                        <button
                          type="button"
                          className="button button-quiet button-compact"
                          onClick={() => void handleDeleteConsumer(consumer)}
                          disabled={busy}
                          aria-label={`删除消费者 ${consumer.name}`}
                        >
                          删除
                        </button>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </section>

        <section className="stream-group-subpanel" aria-labelledby="stream-pending-title">
          <div className="stream-subpanel-heading">
            <div>
              <h4 id="stream-pending-title">Pending 消息</h4>
              <span className="stream-subpanel-hint">最多显示 100 条</span>
            </div>
            <div className="stream-pending-actions">
              <button
                type="button"
                className="button button-quiet button-compact"
                onClick={() =>
                  setSelectedPendingIds(allPendingSelected ? [] : pending.map((entry) => entry.id))
                }
                disabled={busy || pending.length === 0}
              >
                {allPendingSelected ? "取消全选" : "选择当前 Pending"}
              </button>
              <button
                type="button"
                className="button button-secondary button-compact"
                onClick={() => void handleAcknowledge()}
                disabled={busy || selectedPendingIds.length === 0}
              >
                确认选中
              </button>
            </div>
          </div>
          <div className="stream-group-create">
            <label className="field"><span>目标消费者</span>
              <input aria-label="目标消费者" value={claimConsumer} maxLength={256}
                onChange={(event) => setClaimConsumer(event.target.value)} disabled={busy} />
            </label>
            <label className="field"><span>最小空闲时间（毫秒）</span>
              <input aria-label="最小空闲时间（毫秒）" type="number" min="0" step="1" value={claimIdle}
                onChange={(event) => setClaimIdle(event.target.value)} disabled={busy} />
            </label>
            <button type="button" className="button button-secondary" onClick={() => void handleClaim()}
              disabled={busy || detailsLoading || !selectedGroup || !claimConsumer.trim() || selectedPendingIds.length === 0}>
              转移选中 Pending
            </button>
          </div>
          {claimNotice ? <p role="status">{claimNotice}</p> : null}
          {pending.length === 0 ? (
            <p className="stream-empty-state">
              {detailsLoading ? "正在加载 Pending…" : "当前 Group 没有 Pending 消息。"}
            </p>
          ) : (
            <div className="stream-table-wrap">
              <table className="stream-groups-table">
                <thead>
                  <tr>
                    <th aria-label="选择" />
                    <th>ID</th>
                    <th>消费者</th>
                    <th>空闲</th>
                    <th>投递次数</th>
                  </tr>
                </thead>
                <tbody>
                  {pending.map((entry) => (
                    <tr key={entry.id}>
                      <td>
                        <input
                          type="checkbox"
                          aria-label={`选择 Pending ${entry.id}`}
                          checked={selectedPendingIds.includes(entry.id)}
                          onChange={() => togglePending(entry.id)}
                          disabled={busy}
                        />
                      </td>
                      <td><code>{entry.id}</code></td>
                      <td><code>{entry.consumer}</code></td>
                      <td>{formatIdle(entry.idle_ms)}</td>
                      <td>{entry.deliveries}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </section>
      </div>
    </section>
  );
}

export default StreamConsumerGroups;
