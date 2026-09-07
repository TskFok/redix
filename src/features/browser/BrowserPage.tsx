import Select from "../../components/Select";
import { BULK_TASK_FINISHED, type BulkTask } from "../tasks/bulkTaskApi";
import { useAutoRefresh } from "./useAutoRefresh";
import StartBulkDeleteButton from "../tasks/StartBulkDeleteButton";
import { useCallback, useEffect, useRef, useState } from "react";

import { getBrowserKey, getModuleCapabilities, scanKeys } from "../../lib/tauri";
import type { KeyValue, ScanCursor } from "../../lib/types";
import KeyDetails from "./KeyDetails";
import KeyList from "./KeyList";
import AddKey from "./AddKey";
import BulkKeyActions from "./BulkKeyActions";
import BrowserImportExport from "./BrowserImportExport";
import {
  applyScanPage,
  browserErrorMessage,
  initialBrowserPageState,
  type BrowserPageState,
  type ModuleProbeState,
} from "./browserState";

interface BrowserPageProps {
  connectionId: string;
  scanCount?: number;
}

const FILTER_DEBOUNCE_MS = 320;

export function BrowserPage({ connectionId, scanCount = 100 }: BrowserPageProps) {
  const [state, setState] = useState<BrowserPageState>(() => ({
    ...initialBrowserPageState,
  }));
  const [bulkChanged, setBulkChanged] = useState(false);
  const [refreshSeconds, setRefreshSeconds] = useState(0);
  const [detailLoading, setDetailLoading] = useState(false);
  const [detailActionLoading, setDetailActionLoading] = useState(false);
  const [showAddKey, setShowAddKey] = useState(false);
  const [moduleProbe, setModuleProbe] = useState<ModuleProbeState>({
    status: "loading",
    capabilities: null,
  });
  const connectionIdRef = useRef(connectionId);
  connectionIdRef.current = connectionId;
  const mountedRef = useRef(false);
  const scanLoadingRef = useRef(false);
  const scanRequestRef = useRef(0);
  const detailRequestRef = useRef(0);
  const moduleProbeRequestRef = useRef(0);
  const debounceRef = useRef<number | null>(null);
  const skipDebounceForPatternRef = useRef<string | null>(null);
  const normalizedScanCount =
    Number.isInteger(scanCount) && scanCount >= 10 && scanCount <= 1000 ? scanCount : 100;

  const scanPage = useCallback(
    async (
      cursor: ScanCursor,
      requestedPattern: string,
      replace: boolean,
      requestedKeyType: string,
    ) => {
      if (scanLoadingRef.current) {
        return;
      }

      scanLoadingRef.current = true;
      const requestId = scanRequestRef.current + 1;
      scanRequestRef.current = requestId;
      if (replace) {
        detailRequestRef.current += 1;
        setDetailLoading(false);
      }
      setState((current) => ({
        ...current,
        pattern: requestedPattern,
        keyType: requestedKeyType,
        cursor: replace ? 0 : current.cursor,
        nodeFailures: replace ? [] : current.nodeFailures,
        hasMore: replace ? false : current.hasMore,
        keys: replace ? [] : current.keys,
        selectedKey: replace ? null : current.selectedKey,
        selectedKeys: replace ? [] : current.selectedKeys,
        detail: replace ? null : current.detail,
        metadata: replace ? null : current.metadata,
        loading: true,
        error: null,
      }));

      try {
        const page = await scanKeys({
          connection_id: connectionId,
          cursor,
          pattern: requestedPattern,
          count: normalizedScanCount,
          key_type: requestedKeyType || null,
        });
        if (mountedRef.current && scanRequestRef.current === requestId) {
          setState((current) => applyScanPage(current, page, replace, requestedKeyType));
        }
      } catch (caught) {
        if (mountedRef.current && scanRequestRef.current === requestId) {
          setState((current) => ({
            ...current,
            loading: false,
            error: browserErrorMessage(caught, "加载键失败，请稍后重试。"),
          }));
        }
      } finally {
        if (scanRequestRef.current === requestId) {
          scanLoadingRef.current = false;
        }
      }
    },
    [connectionId, normalizedScanCount],
  );

  useEffect(() => {
    mountedRef.current = true;
    scanLoadingRef.current = false;
    skipDebounceForPatternRef.current = "*";
    setDetailActionLoading(false);
    setShowAddKey(false);
    setState({
      ...initialBrowserPageState,
      pattern: "*",
      loading: true,
    });
    void scanPage(0, "*", true, "");

    return () => {
      mountedRef.current = false;
      scanRequestRef.current += 1;
      scanLoadingRef.current = false;
      detailRequestRef.current += 1;
      if (debounceRef.current !== null) {
        window.clearTimeout(debounceRef.current);
        debounceRef.current = null;
      }
    };
  }, [connectionId, scanPage]);

  useEffect(() => {
    const requestId = moduleProbeRequestRef.current + 1;
    moduleProbeRequestRef.current = requestId;
    const requestedConnectionId = connectionId;
    setModuleProbe({ status: "loading", capabilities: null });

    void getModuleCapabilities(requestedConnectionId)
      .then((capabilities) => {
        if (
          mountedRef.current &&
          moduleProbeRequestRef.current === requestId &&
          connectionIdRef.current === requestedConnectionId
        ) {
          setModuleProbe({ status: "ready", capabilities });
        }
      })
      .catch(() => {
        if (
          mountedRef.current &&
          moduleProbeRequestRef.current === requestId &&
          connectionIdRef.current === requestedConnectionId
        ) {
          setModuleProbe({ status: "failed", capabilities: null });
        }
      });

    return () => {
      moduleProbeRequestRef.current += 1;
    };
  }, [connectionId]);

  useEffect(() => {
    if (skipDebounceForPatternRef.current === state.pattern) {
      skipDebounceForPatternRef.current = null;
      return;
    }

    if (debounceRef.current !== null) {
      window.clearTimeout(debounceRef.current);
    }

    const requestedPattern = state.pattern.trim() || "*";
    debounceRef.current = window.setTimeout(() => {
      debounceRef.current = null;
      void scanPage(0, requestedPattern, true, state.keyType);
    }, FILTER_DEBOUNCE_MS);

    return () => {
      if (debounceRef.current !== null) {
        window.clearTimeout(debounceRef.current);
        debounceRef.current = null;
      }
    };
  }, [scanPage, state.pattern]);

  const handlePatternChange = (pattern: string) => {
    setState((current) => ({ ...current, pattern, error: null }));
  };

  const handlePatternKeyDown = (event: React.KeyboardEvent<HTMLInputElement>) => {
    if (event.key !== "Enter") {
      return;
    }

    event.preventDefault();
    const requestedPattern = event.currentTarget.value.trim() || "*";
    if (state.pattern !== requestedPattern) {
      skipDebounceForPatternRef.current = requestedPattern;
    } else {
      skipDebounceForPatternRef.current = null;
    }
    if (debounceRef.current !== null) {
      window.clearTimeout(debounceRef.current);
      debounceRef.current = null;
    }
    void scanPage(0, requestedPattern, true, state.keyType);
  };

  const handleKeyTypeChange = (keyType: string) => {
    if (state.loading) {
      return;
    }
    void scanPage(0, state.pattern.trim() || "*", true, keyType);
  };

  const handleLoadMore = () => {
    if (state.loading || !state.hasMore) {
      return;
    }
    void scanPage(state.cursor, state.pattern.trim() || "*", false, state.keyType);
  };

  const handleRefresh = () => {
    if (state.loading) {
      return;
    }
    setBulkChanged(false);
    void scanPage(0, state.pattern.trim() || "*", true, state.keyType);
  };

  const handleToggleSelect = (key: string) => {
    if (state.loading || detailActionLoading) {
      return;
    }
    setState((current) => ({
      ...current,
      selectedKeys: current.selectedKeys.includes(key)
        ? current.selectedKeys.filter((item) => item !== key)
        : [...current.selectedKeys, key],
    }));
  };

  const handleCreated = () => {
    setShowAddKey(false);
    void scanPage(0, state.pattern.trim() || "*", true, state.keyType);
  };

  useEffect(() => {
    setBulkChanged(false);
    const completed = (event: Event) => {
      const task = (event as CustomEvent<BulkTask>).detail;
      if (task.connection_id !== connectionId) return;
      setState((current) => ({ ...current, selectedKeys: [] }));
      setBulkChanged(true);
    };
    window.addEventListener(BULK_TASK_FINISHED, completed);
    return () => window.removeEventListener(BULK_TASK_FINISHED, completed);
  }, [connectionId]);

  const handleBulkDeleted = () => {
    void scanPage(0, state.pattern.trim() || "*", true, state.keyType);
  };

  const handleImported = async () => {
    await scanPage(0, state.pattern.trim() || "*", true, state.keyType);
  };

  const handleBulkError = (message: string) => {
    setState((current) => ({ ...current, error: message }));
  };

  const handleSelect = async (key: string) => {
    if (state.loading) {
      return;
    }
    const requestId = detailRequestRef.current + 1;
    detailRequestRef.current = requestId;
    setDetailLoading(true);
    setState((current) => ({
      ...current,
      selectedKey: key,
      detail: null,
      metadata: null,
      error: null,
    }));

    try {
      const detail = await getBrowserKey({ connection_id: connectionId, key });
      if (mountedRef.current && detailRequestRef.current === requestId) {
        setState((current) => ({ ...current, detail, error: null }));
      }
    } catch (caught) {
      if (mountedRef.current && detailRequestRef.current === requestId) {
        setState((current) => ({
          ...current,
          detail: null,
          error: browserErrorMessage(caught, "读取键失败，请稍后重试。"),
        }));
      }
    } finally {
      if (mountedRef.current && detailRequestRef.current === requestId) {
        setDetailLoading(false);
      }
    }
  };

  const handleDetailChange = (detail: KeyValue) => {
    if (connectionIdRef.current !== connectionId) {
      return;
    }
    setState((current) => ({
      ...current,
      detail,
      metadata: null,
      error: null,
      keys: current.keys.map((summary) =>
        summary.key === detail.key
          ? { ...summary, key_type: detail.key_type, ttl_ms: detail.ttl_ms }
          : summary,
      ),
    }));
  };

  const handleRenamed = (previousKey: string, detail: KeyValue) => {
    if (connectionIdRef.current !== connectionId) {
      return;
    }
    setState((current) => ({
      ...current,
      selectedKey: current.selectedKey === previousKey ? detail.key : current.selectedKey,
      detail: current.detail?.key === previousKey ? detail : current.detail,
      metadata: null,
      keys: current.keys.map((summary) =>
        summary.key === previousKey
          ? {
              ...summary,
              key: detail.key,
              key_type: detail.key_type,
              ttl_ms: detail.ttl_ms,
            }
          : summary,
      ),
      error: null,
    }));
  };

  const handleDeleted = (key: string) => {
    if (connectionIdRef.current !== connectionId) {
      return;
    }
    detailRequestRef.current += 1;
    setDetailLoading(false);
    setDetailActionLoading(false);
    setState((current) => ({
      ...current,
      keys: current.keys.filter((summary) => summary.key !== key),
      selectedKey: current.selectedKey === key ? null : current.selectedKey,
      selectedKeys: current.selectedKeys.filter((item) => item !== key),
      detail: current.detail?.key === key ? null : current.detail,
      metadata: current.detail?.key === key ? null : current.metadata,
      error: null,
    }));
  };

  const listBusy = state.loading || detailLoading || detailActionLoading;
  useAutoRefresh(refreshSeconds, listBusy || showAddKey || state.selectedKey !== null || state.selectedKeys.length > 0, () => {
    void scanPage(0, state.pattern.trim() || "*", true, state.keyType);
  });
  const arraySupported =
    moduleProbe.status === "ready" && moduleProbe.capabilities.array_supported;
  const vectorSetSupported =
    moduleProbe.status === "ready" && moduleProbe.capabilities.vector_set_supported;

  return (
    <section className="browser-page" aria-labelledby="browser-page-title" aria-busy={listBusy}>
      <div className="page-heading browser-page-heading">
        <div>
          <p className="eyebrow">BROWSER</p>
          <h2 id="browser-page-title">数据浏览</h2>
          <p className="page-description">
            使用游标扫描浏览键，按需读取和编辑 Redis 数据。
          </p>
        </div>
        <span className="browser-connection-id">连接 ID：{connectionId}</span>
      </div>

      {state.error ? (
        <p className="feedback feedback-error" role="alert">
          {state.error}
        </p>
      ) : null}
      {state.nodeFailures.length > 0 && <div className="feedback feedback-error" role="alert"><p>{state.nodeFailures.length} 个节点扫描失败，当前键列表为部分结果。</p><button type="button" className="button button-secondary" disabled={listBusy} onClick={state.hasMore ? handleLoadMore : handleRefresh}>{state.hasMore ? "继续扫描并重试" : "重新扫描并重试"}</button></div>}

      <div className="browser-actions" aria-label="Browser 操作">
        <button
          type="button"
          className="button button-secondary"
          onClick={() => setShowAddKey(true)}
          disabled={listBusy || showAddKey}
        >
          新增键
        </button>
        <button
          type="button"
          className="button button-secondary"
          onClick={handleRefresh}
          disabled={listBusy}
        >
          刷新键列表
        </button>
        {bulkChanged && <p role="status">后台删除已结束，选择已清除；列表可能已变化，请刷新查看最新结果。</p>}
        <label className="field"><span>键列表自动刷新</span><Select value={refreshSeconds} onChange={(event) => setRefreshSeconds(Number(event.target.value))}><option value={0}>关闭</option><option value={2}>每 2 秒</option><option value={5}>每 5 秒</option><option value={10}>每 10 秒</option><option value={30}>每 30 秒</option></Select></label>
        {refreshSeconds > 0 && <small>选择键、查看详情或编辑期间暂停自动刷新。</small>}
        <StartBulkDeleteButton connectionId={connectionId} keys={state.selectedKeys} disabled={listBusy} />
        <BulkKeyActions
          connectionId={connectionId}
          selectedKeys={state.selectedKeys}
          busy={listBusy}
          onDeleted={handleBulkDeleted}
          onError={handleBulkError}
        />
        <BrowserImportExport
          connectionId={connectionId}
          selectedKeys={state.selectedKeys}
          onImported={handleImported}
          disabled={listBusy}
        />
      </div>

      {showAddKey ? (
        <AddKey
          connectionId={connectionId}
          busy={listBusy}
          arraySupported={arraySupported}
          vectorSetSupported={vectorSetSupported}
          onCreated={handleCreated}
          onCancel={() => setShowAddKey(false)}
        />
      ) : null}

      <div className="browser-layout">
        <KeyList
          key={connectionId}
          pattern={state.pattern}
          keyType={state.keyType}
          keys={state.keys}
          selectedKey={state.selectedKey}
          selectedKeys={state.selectedKeys}
          arraySupported={arraySupported}
          vectorSetSupported={vectorSetSupported}
          hasMore={state.hasMore}
          loading={listBusy}
          onPatternChange={handlePatternChange}
          onPatternKeyDown={handlePatternKeyDown}
          onKeyTypeChange={handleKeyTypeChange}
          onSelect={(key) => void handleSelect(key)}
          onToggleSelect={handleToggleSelect}
          onLoadMore={handleLoadMore}
        />
        <KeyDetails
          connectionId={connectionId}
          detail={state.detail}
          metadata={state.metadata}
          loading={detailLoading}
          moduleProbe={moduleProbe}
          onDetailChange={handleDetailChange}
          onMetadataChange={(metadata) =>
            setState((current) => ({ ...current, metadata }))
          }
          onRenamed={handleRenamed}
          onDeleted={handleDeleted}
          onBusyChange={setDetailActionLoading}
        />
      </div>
    </section>
  );
}

export default BrowserPage;
