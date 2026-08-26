import { useEffect, useRef, useState } from "react";

import {
  deleteKey,
  deleteJsonPath,
  getKeyInfo,
  getKey,
  getJsonPath,
  appendJsonArray,
  renameKey,
  setKey,
  setJsonPath,
  setKeyTtl,
} from "../../lib/tauri";
import type {
  JsonMutationResult,
  JsonPathValue,
  KeyInfo,
  KeyValue,
  RedisValue,
} from "../../lib/types";
import {
  browserErrorMessage,
  jsonPathErrorMessage,
  jsonPathUnavailableMessage,
  keyTypeLabel,
  type ModuleProbeState,
} from "./browserState";
import KeyEditor from "./KeyEditor";
import JsonPathEditor, { type JsonPathMutation } from "./JsonPathEditor";
import StreamConsumerGroups from "./StreamConsumerGroups";

interface KeyDetailsProps {
  connectionId: string;
  detail: KeyValue | null;
  metadata?: KeyInfo | null;
  loading: boolean;
  moduleProbe?: ModuleProbeState;
  onDetailChange: (detail: KeyValue) => void;
  onMetadataChange?: (metadata: KeyInfo | null) => void;
  onRenamed?: (previousKey: string, detail: KeyValue) => void;
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
  metadata,
  loading,
  moduleProbe = { status: "loading", capabilities: null },
  onDetailChange,
  onMetadataChange,
  onRenamed,
  onDeleted,
  onBusyChange,
}: KeyDetailsProps) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [jsonPathError, setJsonPathError] = useState<string | null>(null);
  const [renameDraft, setRenameDraft] = useState(detail?.key ?? "");
  const [localInfo, setLocalInfo] = useState<KeyInfo | null>(null);
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
    setJsonPathError(null);
    setRenameDraft(detail?.key ?? "");
    setLocalInfo(null);
    onMetadataChange?.(null);
    onBusyChange?.(false);
  }, [connectionId, detail?.key, onBusyChange]);

  const info = metadata === undefined ? localInfo : metadata;

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

  const handleRename = async () => {
    const nextKey = renameDraft.trim();
    if (nextKey === "") {
      setError("新键名不能为空。");
      return;
    }
    if (nextKey === detail.key) {
      setError("新键名必须与当前键名不同。");
      return;
    }
    const operation = beginOperation(detail.key);
    operationRef.current = operation.token;
    setOperationBusy(operation, true);
    setError(null);
    try {
      const renamed = await renameKey({
        connection_id: operation.connectionId,
        key: operation.key,
        new_key: nextKey,
      });
      if (isCurrent(operation)) {
        setRenameDraft(renamed.key);
        if (onRenamed) {
          onRenamed(operation.key, renamed);
        } else {
          onDetailChange(renamed);
        }
      }
    } catch (caught) {
      if (isCurrent(operation)) {
        setError(browserErrorMessage(caught, "重命名键失败，请稍后重试。"));
      }
    } finally {
      setOperationBusy(operation, false);
    }
  };

  const handleInfo = async () => {
    const operation = beginOperation(detail.key);
    operationRef.current = operation.token;
    setOperationBusy(operation, true);
    setError(null);
    try {
      const nextInfo = await getKeyInfo({
        connection_id: operation.connectionId,
        key: operation.key,
      });
      if (isCurrent(operation)) {
        setLocalInfo(nextInfo);
        onMetadataChange?.(nextInfo);
      }
    } catch (caught) {
      if (isCurrent(operation)) {
        setError(browserErrorMessage(caught, "读取元数据失败，请稍后重试。"));
      }
    } finally {
      setOperationBusy(operation, false);
    }
  };

  const handleJsonPathRead = async (path: string): Promise<JsonPathValue> => {
    const operation = beginOperation(detail.key);
    operationRef.current = operation.token;
    setOperationBusy(operation, true);
    setJsonPathError(null);
    try {
      const result = await getJsonPath({
        connection_id: operation.connectionId,
        key: operation.key,
        path,
      });
      if (!isCurrent(operation)) {
        throw new Error("stale-json-path-read");
      }
      return result;
    } catch (caught) {
      if (isCurrent(operation)) {
        setJsonPathError(
          jsonPathErrorMessage(caught, "读取 JSON Path 失败，请稍后重试。"),
        );
      }
      throw caught;
    } finally {
      setOperationBusy(operation, false);
    }
  };

  const handleJsonPathMutate = async (
    mutation: JsonPathMutation,
  ): Promise<JsonMutationResult> => {
    const operation = beginOperation(detail.key);
    operationRef.current = operation.token;
    setOperationBusy(operation, true);
    setJsonPathError(null);
    try {
      const result =
        mutation.kind === "set"
          ? await setJsonPath({
              connection_id: operation.connectionId,
              key: operation.key,
              path: mutation.path,
              value: mutation.value,
            })
          : mutation.kind === "append"
            ? await appendJsonArray({
                connection_id: operation.connectionId,
                key: operation.key,
                path: mutation.path,
                values: mutation.values,
              })
            : await deleteJsonPath({
                connection_id: operation.connectionId,
                key: operation.key,
                path: mutation.path,
              });
      if (!isCurrent(operation)) {
        throw new Error("stale-json-path-mutation");
      }
      const refreshed = await refreshDetail(operation);
      if (refreshed && isCurrent(operation)) {
        onDetailChange(refreshed);
      }
      return result;
    } catch (caught) {
      if (isCurrent(operation)) {
        setJsonPathError(
          jsonPathErrorMessage(caught, "JSON Path 操作失败，请稍后重试。"),
        );
      }
      throw caught;
    } finally {
      setOperationBusy(operation, false);
    }
  };

  const isJsonDetail = "Json" in detail.value;
  const jsonPathUnsupported = isJsonDetail
    ? jsonPathUnavailableMessage(moduleProbe)
    : null;
  const showJsonPathEditor =
    isJsonDetail &&
    moduleProbe.status === "ready" &&
    moduleProbe.capabilities.json_supported;

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
      <div className="detail-rename-row">
        <label className="field">
          <span>重命名</span>
          <input
            aria-label="新键名"
            value={renameDraft}
            onChange={(event) => {
              setRenameDraft(event.target.value);
              setError(null);
            }}
            disabled={busy}
            spellCheck={false}
          />
        </label>
        <button type="button" className="button button-secondary" onClick={() => void handleRename()} disabled={busy}>
          重命名
        </button>
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
      <div className="detail-info-actions">
        <button type="button" className="button button-quiet" onClick={() => void handleInfo()} disabled={busy}>
          刷新元数据
        </button>
      </div>
      {info ? (
        <dl className="detail-info-grid" aria-label="详细键元数据">
          <div>
            <dt>逻辑大小</dt>
            <dd>{info.size === null ? "—" : info.size}</dd>
          </div>
          <div>
            <dt>内存占用</dt>
            <dd>{info.memory_bytes === null ? "—" : `${info.memory_bytes} B`}</dd>
          </div>
          <div>
            <dt>编码</dt>
            <dd>{info.encoding ?? "—"}</dd>
          </div>
          <div>
            <dt>空闲时间</dt>
            <dd>{info.idle_seconds === null ? "—" : `${info.idle_seconds} s`}</dd>
          </div>
        </dl>
      ) : null}

      <KeyEditor
        key={JSON.stringify([detail.key, detail.ttl_ms, detail.value])}
        value={detail.value}
        ttlMs={detail.ttl_ms}
        busy={busy}
        error={error}
        onSave={handleSave}
        onDelete={handleDelete}
        onSetTtl={handleSetTtl}
      />
      {jsonPathUnsupported ? (
        <p className="json-path-unavailable" role="status">
          {jsonPathUnsupported}
        </p>
      ) : null}
      {showJsonPathEditor && "Json" in detail.value ? (
        <JsonPathEditor
          key={JSON.stringify([
            connectionId,
            detail.key,
            detail.ttl_ms,
            detail.value,
            moduleProbe.status,
            moduleProbe.capabilities.json_supported,
            moduleProbe.capabilities.json_version,
          ])}
          value={detail.value.Json.value}
          busy={busy}
          error={jsonPathError}
          onRead={handleJsonPathRead}
          onMutate={handleJsonPathMutate}
        />
      ) : null}
      {detail.key_type.toLowerCase() === "stream" ? (
        <StreamConsumerGroups
          key={`${connectionId}:${detail.key}`}
          connectionId={connectionId}
          streamKey={detail.key}
        />
      ) : null}
    </section>
  );
}

export default KeyDetails;
