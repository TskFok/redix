import { useEffect, useState } from "react";

import {
  deleteKey,
  getKey,
  setKey,
  setKeyTtl,
} from "../../lib/tauri";
import type { KeyValue, RedisValue } from "../../lib/types";
import { browserErrorMessage, keyTypeLabel } from "./browserState";
import KeyEditor from "./KeyEditor";

interface KeyDetailsProps {
  connectionId: string;
  detail: KeyValue | null;
  loading: boolean;
  onDetailChange: (detail: KeyValue) => void;
  onDeleted: (key: string) => void;
  onBusyChange?: (busy: boolean) => void;
}

export function KeyDetails({
  connectionId,
  detail,
  loading,
  onDetailChange,
  onDeleted,
  onBusyChange,
}: KeyDetailsProps) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    setError(null);
  }, [detail?.key]);

  if (!detail) {
    return (
      <section className="browser-detail-panel browser-empty-details" aria-labelledby="key-details-title">
        <div>
          <p className="eyebrow">DETAILS</p>
          <h2 id="key-details-title">键详情</h2>
        </div>
        <p className="empty-detail-message" role={loading ? "status" : undefined}>
          {loading ? "正在读取键详情…" : "请选择一个键查看详情"}
        </p>
      </section>
    );
  }

  const refreshDetail = async () => {
    const refreshed = await getKey({ connection_id: connectionId, key: detail.key });
    onDetailChange(refreshed);
  };

  const handleSave = async (value: RedisValue) => {
    setBusy(true);
    onBusyChange?.(true);
    setError(null);
    try {
      await setKey({ connection_id: connectionId, key: detail.key, value });
      await refreshDetail();
    } catch (caught) {
      setError(browserErrorMessage(caught, "保存键失败，请稍后重试。"));
    } finally {
      setBusy(false);
      onBusyChange?.(false);
    }
  };

  const handleDelete = async () => {
    setBusy(true);
    onBusyChange?.(true);
    setError(null);
    try {
      await deleteKey({ connection_id: connectionId, key: detail.key });
      onDeleted(detail.key);
    } catch (caught) {
      setError(browserErrorMessage(caught, "删除键失败，请稍后重试。"));
    } finally {
      setBusy(false);
      onBusyChange?.(false);
    }
  };

  const handleSetTtl = async (ttlMs: number) => {
    setBusy(true);
    onBusyChange?.(true);
    setError(null);
    try {
      await setKeyTtl({ connection_id: connectionId, key: detail.key, ttl_ms: ttlMs });
      await refreshDetail();
    } catch (caught) {
      setError(browserErrorMessage(caught, "设置 TTL 失败，请稍后重试。"));
    } finally {
      setBusy(false);
      onBusyChange?.(false);
    }
  };

  return (
    <section
      className="browser-detail-panel"
      aria-labelledby="key-details-title"
      aria-busy={busy}
    >
      <div className="browser-panel-heading">
        <div>
          <p className="eyebrow">DETAILS</p>
          <h2 id="key-details-title">键详情</h2>
        </div>
        <span className="detail-type">{keyTypeLabel(detail.key_type)}</span>
      </div>

      <div className="detail-key-line">
        <span>键名</span>
        <code title={detail.key}>{detail.key}</code>
      </div>
      <div className="detail-metadata" aria-label="键元数据">
        <div>
          <span>类型</span>
          <strong>{keyTypeLabel(detail.key_type)}</strong>
        </div>
        <div>
          <span>TTL</span>
          <strong>{detail.ttl_ms < 0 ? "永久" : `${detail.ttl_ms} ms`}</strong>
        </div>
      </div>

      <KeyEditor
        value={detail.value}
        ttlMs={detail.ttl_ms}
        busy={busy}
        error={error}
        onSave={handleSave}
        onDelete={handleDelete}
        onSetTtl={handleSetTtl}
      />
    </section>
  );
}

export default KeyDetails;
