import { useEffect, useRef, useState } from "react";

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

interface OperationContext {
  token: number;
  connectionId: string;
  key: string;
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
  const mountedRef = useRef(false);
  const operationRef = useRef(0);
  const currentConnectionRef = useRef(connectionId);
  const currentKeyRef = useRef(detail?.key ?? null);
  currentConnectionRef.current = connectionId;
  currentKeyRef.current = detail?.key ?? null;

  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
      operationRef.current += 1;
    };
  }, []);

  useEffect(() => {
    operationRef.current += 1;
    setBusy(false);
    setError(null);
    onBusyChange?.(false);
  }, [connectionId, detail?.key, onBusyChange]);

  const isCurrent = (operation: OperationContext) =>
    mountedRef.current &&
    operationRef.current === operation.token &&
    currentConnectionRef.current === operation.connectionId &&
    currentKeyRef.current === operation.key;

  const beginOperation = (key: string): OperationContext => ({
    token: operationRef.current + 1,
    connectionId,
    key,
  });

  const setOperationBusy = (operation: OperationContext, nextBusy: boolean) => {
    if (!isCurrent(operation)) {
      return;
    }
    setBusy(nextBusy);
    onBusyChange?.(nextBusy);
  };

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

  const refreshDetail = async (operation: OperationContext) => {
    if (!isCurrent(operation)) {
      return null;
    }
    const refreshed = await getKey({
      connection_id: operation.connectionId,
      key: operation.key,
    });
    return isCurrent(operation) ? refreshed : null;
  };

  const handleSave = async (value: RedisValue) => {
    const operation = beginOperation(detail.key);
    operationRef.current = operation.token;
    setOperationBusy(operation, true);
    setError(null);
    try {
      await setKey({ connection_id: operation.connectionId, key: operation.key, value });
      const refreshed = await refreshDetail(operation);
      if (refreshed && isCurrent(operation)) {
        onDetailChange(refreshed);
      }
    } catch (caught) {
      if (isCurrent(operation)) {
        setError(browserErrorMessage(caught, "保存键失败，请稍后重试。"));
      }
    } finally {
      setOperationBusy(operation, false);
    }
  };

  const handleDelete = async () => {
    if (!window.confirm(`确定删除键“${detail.key}”吗？`)) {
      return;
    }
    const operation = beginOperation(detail.key);
    operationRef.current = operation.token;
    setOperationBusy(operation, true);
    setError(null);
    try {
      await deleteKey({ connection_id: operation.connectionId, key: operation.key });
      if (isCurrent(operation)) {
        onDeleted(operation.key);
      }
    } catch (caught) {
      if (isCurrent(operation)) {
        setError(browserErrorMessage(caught, "删除键失败，请稍后重试。"));
      }
    } finally {
      setOperationBusy(operation, false);
    }
  };

  const handleSetTtl = async (ttlMs: number) => {
    const operation = beginOperation(detail.key);
    operationRef.current = operation.token;
    setOperationBusy(operation, true);
    setError(null);
    try {
      await setKeyTtl({
        connection_id: operation.connectionId,
        key: operation.key,
        ttl_ms: ttlMs,
      });
      if (!isCurrent(operation)) {
        return;
      }
      if (ttlMs === 0) {
        onDeleted(operation.key);
        return;
      }
      const refreshed = await refreshDetail(operation);
      if (refreshed && isCurrent(operation)) {
        onDetailChange(refreshed);
      }
    } catch (caught) {
      if (isCurrent(operation)) {
        setError(browserErrorMessage(caught, "设置 TTL 失败，请稍后重试。"));
      }
    } finally {
      setOperationBusy(operation, false);
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
