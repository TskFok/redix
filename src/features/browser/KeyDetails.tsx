import { useEffect, useRef, useState } from "react";
import { useConfirmDialog } from "../../components/useConfirmDialog";

import {
  deleteKey,
  deleteJsonPath,
  getKeySearchIndexes,
  getKeyInfo,
  getBrowserKey,
  getJsonPath,
  appendJsonArray,
  renameBrowserKey,
  setKey,
  setJsonPath,
  setKeyTtl,
} from "../../lib/tauri";
import type {
  JsonMutationResult,
  JsonPathValue,
  KeyInfo,
  KeyValue,
  KeySearchIndexSummary,
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
import ArrayDetails from "./ArrayDetails";
import JsonPathEditor, { type JsonPathMutation } from "./JsonPathEditor";
import StreamConsumerGroups from "./StreamConsumerGroups";
import VectorSetDetails from "./VectorSetDetails";
import CollectionDetails from "./CollectionDetails";
import StreamEntries from "./StreamEntries";
import StringValueEditor from "./StringValueEditor";
import type { CollectionKind } from "./collectionApi";
import { searchCapabilityState } from "../search/searchState";

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

function isRootJsonPath(path: string): boolean {
  const normalized = path.trim();
  return normalized === "$" || normalized === ".";
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
  const [childBusy, setChildBusy] = useState(false);
  const { confirm, confirmationDialog } = useConfirmDialog(
    JSON.stringify([connectionId, detail?.key, loading, busy, childBusy]),
  );
  const [error, setError] = useState<string | null>(null);
  const [jsonPathError, setJsonPathError] = useState<string | null>(null);
  const [searchIndexes, setSearchIndexes] = useState<KeySearchIndexSummary[]>([]);
  const [searchIndexesLoading, setSearchIndexesLoading] = useState(false);
  const [searchIndexesError, setSearchIndexesError] = useState<string | null>(null);
  const [renameDraft, setRenameDraft] = useState(detail?.key ?? "");
  const [moduleTtlDraft, setModuleTtlDraft] = useState("");
  const [localInfo, setLocalInfo] = useState<KeyInfo | null>(null);
  const mountedRef = useRef(false);
  const operationRef = useRef(0);
  const searchIndexesRequestRef = useRef(0);
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
    setChildBusy(false);
    setError(null);
    setJsonPathError(null);
    setRenameDraft(detail?.key ?? "");
    setModuleTtlDraft(detail && detail.ttl_ms >= 0 ? String(detail.ttl_ms) : "");
    setLocalInfo(null);
    onMetadataChange?.(null);
    onBusyChange?.(false);
  }, [connectionId, detail?.key, onBusyChange]);

  const uiBusy = busy || childBusy;
  const handleChildBusy = (nextBusy: boolean) => {
    setChildBusy(nextBusy);
    onBusyChange?.(nextBusy);
  };

  const info = metadata === undefined ? localInfo : metadata;
  const detailKeyType = detail?.key_type.trim().toLowerCase() ?? "";
  const searchableKey =
    detailKeyType === "hash" ||
    detailKeyType === "json" ||
    detailKeyType === "rejson-rl" ||
    detailKeyType === "rejson-rs";
  const searchCapability = searchCapabilityState(moduleProbe);
  const searchSupported = searchCapability.status === "ready";

  useEffect(() => {
    const requestId = searchIndexesRequestRef.current + 1;
    searchIndexesRequestRef.current = requestId;
    setSearchIndexes([]);
    setSearchIndexesError(null);
    setSearchIndexesLoading(false);

    if (!detail || !searchableKey || !searchSupported) {
      return;
    }

    setSearchIndexesLoading(true);
    void getKeySearchIndexes({ connection_id: connectionId, key: detail.key })
      .then((indexes) => {
        if (
          mountedRef.current &&
          searchIndexesRequestRef.current === requestId
        ) {
          setSearchIndexes(indexes);
        }
      })
      .catch((caught) => {
        if (
          mountedRef.current &&
          searchIndexesRequestRef.current === requestId
        ) {
          setSearchIndexesError(
            browserErrorMessage(caught, "读取 RedisSearch 索引失败，请稍后重试。"),
          );
        }
      })
      .finally(() => {
        if (searchIndexesRequestRef.current === requestId) {
          setSearchIndexesLoading(false);
        }
      });

    return () => {
      searchIndexesRequestRef.current += 1;
    };
  }, [connectionId, detail?.key, searchableKey, searchSupported]);

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
    const refreshed = await getBrowserKey({
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
    if (uiBusy || loading) {
      return;
    }
    const operation = beginOperation(detail.key);
    if (!await confirm(`确定删除键“${operation.key}”吗？此操作无法撤销。`)) {
      return;
    }
    if (
      !mountedRef.current ||
      currentConnectionRef.current !== operation.connectionId ||
      currentKeyRef.current !== operation.key ||
      operationRef.current !== operation.token - 1
    ) {
      return;
    }
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
      const renamed = await renameBrowserKey({
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
        return result;
      }
      if (
        mutation.kind === "delete" &&
        isRootJsonPath(mutation.path) &&
        (result.affected > 0 || result.ttl_ms === -2)
      ) {
        onDeleted(operation.key);
        return result;
      }
      try {
        const refreshed = await refreshDetail(operation);
        if (refreshed && isCurrent(operation)) {
          onDetailChange(refreshed);
        }
      } catch {
        return result;
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
  const isArrayDetail = "Array" in detail.value;
  const isVectorSetDetail = "VectorSet" in detail.value;
  const collectionKind = ["hash", "list", "set", "zset"].includes(detailKeyType) ? detailKeyType as CollectionKind : null;
  const isStreamDetail = detailKeyType === "stream";
  const isStringDetail = detailKeyType === "string";
  const isModuleDetail = isArrayDetail || isVectorSetDetail || collectionKind !== null || isStreamDetail || isStringDetail;
  const arraySummary = "Array" in detail.value ? detail.value.Array : null;
  const vectorSetSummary = "VectorSet" in detail.value ? detail.value.VectorSet : null;

  return (
    <section
      className="browser-detail-panel"
      aria-labelledby="key-details-title"
      aria-busy={uiBusy}
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
            disabled={uiBusy}
            spellCheck={false}
          />
        </label>
        <button type="button" className="button button-secondary" onClick={() => void handleRename()} disabled={uiBusy}>
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
        <button type="button" className="button button-quiet" onClick={() => void handleInfo()} disabled={uiBusy}>
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

      {isModuleDetail ? (
        <div className="module-common-actions">
          <div className="ttl-editor">
            <label className="field">
              <span>TTL（毫秒）</span>
              <input
                type="number"
                min="0"
                step="1"
                inputMode="numeric"
                value={moduleTtlDraft}
                onChange={(event) => setModuleTtlDraft(event.target.value)}
                placeholder={detail.ttl_ms < 0 ? "当前为永久" : undefined}
                disabled={uiBusy}
              />
            </label>
            <button
              type="button"
              className="button button-secondary"
              onClick={() => {
                const parsed = moduleTtlDraft.trim() === "" ? Number.NaN : Number(moduleTtlDraft);
                if (!Number.isInteger(parsed) || parsed < 0) {
                  setError("TTL 必须是大于等于 0 的整数毫秒。");
                  return;
                }
                void handleSetTtl(parsed);
              }}
              disabled={uiBusy}
            >
              设置 TTL
            </button>
          </div>
          <button type="button" className="button button-danger" onClick={() => void handleDelete()} disabled={uiBusy}>
            {uiBusy ? "处理中…" : isStringDetail ? "删除" : "删除整个键"}
          </button>
        </div>
      ) : (
        <KeyEditor
          key={JSON.stringify([detail.key, detail.ttl_ms, detail.value])}
          value={detail.value}
          ttlMs={detail.ttl_ms}
          busy={uiBusy}
          error={error}
          onSave={handleSave}
          onDelete={handleDelete}
          onSetTtl={handleSetTtl}
        />
      )}
      {confirmationDialog}
      {isModuleDetail && error ? <p className="feedback feedback-error" role="alert">{error}</p> : null}
      {isStringDetail && <StringValueEditor connectionId={connectionId} keyName={detail.key} disabled={busy} onBusyChange={handleChildBusy} onSaved={(result) => {
        // Keep the raw payload in the dedicated editor; the Browser DTO stays a preview.
        onDetailChange({ ...detail, ttl_ms: result.ttl_ms });
        setLocalInfo(null); onMetadataChange?.(null);
      }} />}
      {collectionKind && <CollectionDetails key={JSON.stringify([connectionId, detail.key, collectionKind])} connectionId={connectionId} keyName={detail.key} kind={collectionKind} disabled={busy} onBusyChange={handleChildBusy} />}
      {isStreamDetail && <StreamEntries key={JSON.stringify([connectionId, detail.key])} connectionId={connectionId} streamKey={detail.key} disabled={busy} onBusyChange={handleChildBusy} />}
      {isArrayDetail ? (
        <ArrayDetails
          key={`${connectionId}:${detail.key}`}
          connectionId={connectionId}
          keyName={detail.key}
          initialSummary={arraySummary ? {
            key: detail.key,
            length: arraySummary.length,
            count: arraySummary.count,
            next_index: "0",
          } : undefined}
          disabled={busy}
          onBusyChange={handleChildBusy}
        />
      ) : null}
      {isVectorSetDetail ? (
        <VectorSetDetails
          key={`${connectionId}:${detail.key}`}
          connectionId={connectionId}
          keyName={detail.key}
          initialSummary={vectorSetSummary ? {
            key: detail.key,
            total: vectorSetSummary.total,
            dimension: vectorSetSummary.dimension,
            quantization: vectorSetSummary.quantization,
          } : undefined}
          disabled={busy}
          onBusyChange={handleChildBusy}
        />
      ) : null}
      {jsonPathUnsupported ? (
        <p className="json-path-unavailable" role="status">
          {jsonPathUnsupported}
        </p>
      ) : null}
      {showJsonPathEditor && "Json" in detail.value ? (
        <JsonPathEditor
          key={JSON.stringify([connectionId, detail.key])}
          value={detail.value.Json.value}
          busy={busy}
          error={jsonPathError}
          confirmRootDelete={(path) =>
            !isRootJsonPath(path)
              || window.confirm(`确定删除整个 JSON 键“${detail.key}”吗？`)
          }
          onRead={handleJsonPathRead}
          onMutate={handleJsonPathMutate}
        />
      ) : null}
      {searchableKey && searchSupported ? (
        <section className="key-search-indexes" aria-labelledby="key-search-indexes-title">
          <div className="stream-subpanel-heading">
            <div>
              <p className="eyebrow">REDISEARCH</p>
              <h3 id="key-search-indexes-title">所属 RedisSearch 索引</h3>
            </div>
            {searchIndexesLoading ? <span>读取中…</span> : null}
          </div>
          {searchIndexesError ? (
            <p className="inline-error" role="alert">{searchIndexesError}</p>
          ) : searchIndexes.length > 0 ? (
            <ul className="key-search-index-list">
              {searchIndexes.map((index) => (
                <li key={index.name}>
                  <code>{index.name}</code>
                  <span>
                    {index.key_type}
                    {index.prefixes.length > 0 ? ` · ${index.prefixes.join(", ")}` : " · 全部键"}
                  </span>
                </li>
              ))}
            </ul>
          ) : searchIndexesLoading ? null : (
            <p className="empty-state-compact">该键不属于已发现的 RedisSearch 索引。</p>
          )}
        </section>
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
