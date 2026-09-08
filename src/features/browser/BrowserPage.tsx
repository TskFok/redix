import Select from "../../components/Select";
import Toast from "../../components/Toast";
import { useTransientFeedback } from "../../components/useTransientFeedback";
import { BULK_TASK_FINISHED, type BulkTask } from "../tasks/bulkTaskApi";
import { useAutoRefresh } from "./useAutoRefresh";
import StartBulkDeleteButton from "../tasks/StartBulkDeleteButton";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import { getBrowserKey, getModuleCapabilities, scanKeys } from "../../lib/tauri";
import type { ConnectionProfile, KeyValue, ScanCursor } from "../../lib/types";
import KeyDetails from "./KeyDetails";
import KeyList from "./KeyList";
import AddKey from "./AddKey";
import BulkKeyActions from "./BulkKeyActions";
import BrowserImportExport from "./BrowserImportExport";
import BrowserDatabaseSelect from "./BrowserDatabaseSelect";
import {
  applyCreatedKey,
  applyScanPage,
  applyKeyTypeFilter,
  browserErrorMessage,
  filterKeysByType,
  initialBrowserPageState,
  type BrowserPageState,
  type ModuleProbeState,
} from "./browserState";

interface BrowserPageProps {
  connectionId: string;
  activeDatabase?: number;
  isCluster?: boolean;
  onProfileChanged?: (profile: ConnectionProfile) => void;
  databaseSwitching?: boolean;
  onDatabaseSwitchingChange?: (switching: boolean) => void;
  scanCount?: number;
  active?: boolean;
}

const FILTER_DEBOUNCE_MS = 320;

export function BrowserPage({
  connectionId,
  activeDatabase = 0,
  isCluster = false,
  onProfileChanged,
  databaseSwitching: externalDatabaseSwitching = false,
  onDatabaseSwitchingChange,
  scanCount = 100,
  active = true,
}: BrowserPageProps) {
  const [state, setState] = useState<BrowserPageState>(() => ({
    ...initialBrowserPageState,
  }));
  const [bulkFeedback, setBulkFeedback, bulkFeedbackToken] = useTransientFeedback<{
    kind: "success" | "error";
    message: string;
  }>();
  const [refreshSeconds, setRefreshSeconds] = useState(0);
  const [databaseCountsRefreshToken, setDatabaseCountsRefreshToken] = useState(0);
  const [detailLoading, setDetailLoading] = useState(false);
  const [detailActionLoading, setDetailActionLoading] = useState(false);
  const [localDatabaseSwitching, setDatabaseSwitching] = useState(false);
  const databaseSwitching = localDatabaseSwitching || externalDatabaseSwitching;
  const [bulkActionLoading, setBulkActionLoading] = useState(false);
  const [fileActionLoading, setFileActionLoading] = useState(false);
  const [bulkStartLoading, setBulkStartLoading] = useState(false);
  const databaseSwitchingRef = useRef(false);
  databaseSwitchingRef.current = databaseSwitching;
  const [showAddKey, setShowAddKey] = useState(false);
  const [moduleProbe, setModuleProbe] = useState<ModuleProbeState>({
    status: "loading",
    capabilities: null,
  });
  const connectionIdRef = useRef(connectionId);
  connectionIdRef.current = connectionId;
  const mountedRef = useRef(false);
  const scanLoadingRef = useRef(false);
  const deferredScanRef = useRef<{ cursor: ScanCursor; pattern: string; replace: boolean } | null>(null);
  const scanRequestRef = useRef(0);
  const detailRequestRef = useRef(0);
  const moduleProbeRequestRef = useRef(0);
  const debounceRef = useRef<number | null>(null);
  const skipDebounceForPatternRef = useRef<string | null>(null);
  const scannedPatternRef = useRef("*");
  const keyTypeRef = useRef("");
  const visibleKeys = useMemo(() => filterKeysByType(state.keys, state.keyType), [state.keys, state.keyType]);
  const normalizedScanCount =
    Number.isInteger(scanCount) && scanCount >= 10 && scanCount <= 10000 ? scanCount : 100;
  const scanCountRef = useRef(normalizedScanCount);
  scanCountRef.current = normalizedScanCount;

  const refreshDatabaseCounts = useCallback(() => {
    if (mountedRef.current) {
      setDatabaseCountsRefreshToken((current) => current + 1);
    }
  }, []);

  const scanPage = useCallback(
    async (
      cursor: ScanCursor,
      requestedPattern: string,
      replace: boolean,
    ) => {
      if (!mountedRef.current) return;
      if (databaseSwitchingRef.current) {
        deferredScanRef.current = { cursor, pattern: requestedPattern, replace };
        return;
      }
      if (scanLoadingRef.current) return;

      deferredScanRef.current = null;
      scanLoadingRef.current = true;
      scannedPatternRef.current = requestedPattern;
      const requestedKeyType = keyTypeRef.current;
      const requestedScanCount = scanCountRef.current;
      const requestId = scanRequestRef.current + 1;
      scanRequestRef.current = requestId;
      if (replace) {
        detailRequestRef.current += 1;
        setDetailLoading(false);
      }
      setState((current) => ({
        ...current,
        pattern: requestedPattern,
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
        let nextCursor = cursor;
        let replacePage = replace;
        while (mountedRef.current && scanRequestRef.current === requestId) {
          const page = await scanKeys({
            connection_id: connectionId,
            cursor: nextCursor,
            pattern: requestedPattern,
            count: requestedScanCount,
            key_type: null,
          });
          if (!mountedRef.current || scanRequestRef.current !== requestId) return;

          // SCAN can return no matches while later batches still contain keys.
          // Pause on node failures or a stalled cursor so retries stay user-driven.
          const continueScanning = filterKeysByType(page.keys, requestedKeyType).length === 0 && page.has_more
            && page.node_failures.length === 0 && page.cursor !== nextCursor;
          const shouldReplace = replacePage;
          setState((current) => ({
            ...applyScanPage(current, page, shouldReplace),
            loading: continueScanning,
          }));
          if (!continueScanning) break;
          nextCursor = page.cursor;
          replacePage = false;
        }
        if (
          replace && mountedRef.current && scanRequestRef.current === requestId
          && connectionIdRef.current === connectionId
        ) {
          refreshDatabaseCounts();
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
    [connectionId, refreshDatabaseCounts],
  );

  const handleDatabaseSwitching = useCallback((switching: boolean) => {
    onDatabaseSwitchingChange?.(switching);
    if (!mountedRef.current) return;
    databaseSwitchingRef.current = switching;
    setDatabaseSwitching(switching);
    if (switching && debounceRef.current !== null) {
      window.clearTimeout(debounceRef.current);
      debounceRef.current = null;
    }
  }, [onDatabaseSwitchingChange]);

  useEffect(() => {
    mountedRef.current = true;
    scanLoadingRef.current = false;
    keyTypeRef.current = "";
    skipDebounceForPatternRef.current = "*";
    setDetailActionLoading(false);
    setShowAddKey(false);
    setState({
      ...initialBrowserPageState,
      pattern: "*",
      loading: true,
    });
    void scanPage(0, "*", true);

    return () => {
      mountedRef.current = false;
      scanRequestRef.current += 1;
      scanLoadingRef.current = false;
      deferredScanRef.current = null;
      detailRequestRef.current += 1;
      if (debounceRef.current !== null) {
        window.clearTimeout(debounceRef.current);
        debounceRef.current = null;
      }
    };
  }, [connectionId, scanPage]);

  useEffect(() => {
    const pending = deferredScanRef.current;
    if (!databaseSwitching && pending) {
      deferredScanRef.current = null;
      void scanPage(pending.cursor, pending.pattern, pending.replace);
    }
  }, [databaseSwitching, scanPage]);

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
      void scanPage(0, requestedPattern, true);
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
    void scanPage(0, requestedPattern, true);
  };

  const handleKeyTypeChange = (keyType: string) => {
    if (state.loading || detailLoading || detailActionLoading) {
      return;
    }
    keyTypeRef.current = keyType;
    detailRequestRef.current += 1;
    setState((current) => applyKeyTypeFilter(current, keyType));
  };

  const handleLoadMore = () => {
    if (state.loading || !state.hasMore) {
      return;
    }
    void scanPage(state.cursor, state.pattern.trim() || "*", false);
  };

  const handleRefresh = () => {
    if (state.loading) {
      return;
    }
    setBulkFeedback(null);
    void scanPage(0, state.pattern.trim() || "*", true);
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

  const handleCreated = (detail: KeyValue) => {
    if (!mountedRef.current || connectionIdRef.current !== connectionId) {
      return;
    }
    setShowAddKey(false);
    setState((current) => applyCreatedKey(current, detail));
    refreshDatabaseCounts();
  };

  useEffect(() => {
    setBulkFeedback(null);
    const completed = (event: Event) => {
      const task = (event as CustomEvent<BulkTask>).detail;
      if (task.connection_id !== connectionId) return;
      setState((current) => ({ ...current, selectedKeys: [] }));
      setBulkFeedback({
        kind: task.status === "completed" ? "success" : "error",
        message: "后台删除已结束，选择已清除；列表可能已变化，请刷新查看最新结果。",
      });
      refreshDatabaseCounts();
    };
    window.addEventListener(BULK_TASK_FINISHED, completed);
    return () => {
      window.removeEventListener(BULK_TASK_FINISHED, completed);
    };
  }, [connectionId, refreshDatabaseCounts, setBulkFeedback]);

  const handleBulkDeleted = () => {
    void scanPage(0, state.pattern.trim() || "*", true);
  };

  const handleImported = async () => {
    await scanPage(0, state.pattern.trim() || "*", true);
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
    setState((current) => applyKeyTypeFilter({
      ...current,
      detail,
      metadata: null,
      error: null,
      keys: current.keys.map((summary) =>
        summary.key === detail.key
          ? { ...summary, key_type: detail.key_type, ttl_ms: detail.ttl_ms }
          : summary,
      ),
    }, current.keyType));
    refreshDatabaseCounts();
  };

  const handleRenamed = (previousKey: string, detail: KeyValue) => {
    if (connectionIdRef.current !== connectionId) {
      return;
    }
    setState((current) => applyKeyTypeFilter({
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
    }, current.keyType));
    refreshDatabaseCounts();
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
    refreshDatabaseCounts();
  };

  const listBusy = state.loading || detailLoading || detailActionLoading || databaseSwitching
    || bulkActionLoading || fileActionLoading || bulkStartLoading;
  useAutoRefresh(refreshSeconds, !active || listBusy || showAddKey || state.selectedKey !== null || state.selectedKeys.length > 0, () => {
    void scanPage(0, state.pattern.trim() || "*", true);
  });
  const arraySupported =
    moduleProbe.status === "ready" && moduleProbe.capabilities.array_supported;
  const vectorSetSupported =
    moduleProbe.status === "ready" && moduleProbe.capabilities.vector_set_supported;

  return (
    <section className="browser-page" aria-labelledby="browser-page-title" aria-busy={listBusy}
      hidden={!active} style={active ? undefined : { display: "none" }}>
      <h2 id="browser-page-title" className="sr-only">数据浏览</h2>

      {state.error ? <Toast
        kind="error"
        message={state.error}
        onClose={() => setState((current) => ({ ...current, error: null }))}
        resetKey={state}
      /> : null}
      {state.nodeFailures.length > 0 && <div className="feedback feedback-error" role="alert"><p>{state.nodeFailures.length} 个节点扫描失败，当前键列表为部分结果。</p><button type="button" className="button button-secondary" disabled={listBusy} onClick={state.hasMore ? handleLoadMore : handleRefresh}>{state.hasMore ? "继续扫描并重试" : "重新扫描并重试"}</button></div>}

      <div className="browser-actions" aria-label="Browser 操作">
        {onProfileChanged && <BrowserDatabaseSelect
          connectionId={connectionId}
          activeDatabase={activeDatabase}
          isCluster={isCluster}
          refreshToken={databaseCountsRefreshToken}
          disabled={listBusy || showAddKey || (state.pattern.trim() || "*") !== scannedPatternRef.current}
          onProfileChanged={onProfileChanged}
          onSwitchingChange={handleDatabaseSwitching}
        />}
        <button
          type="button"
          className="button button-secondary"
          onClick={() => setShowAddKey(true)}
          disabled={listBusy}
          aria-haspopup="dialog"
          aria-expanded={showAddKey}
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
        {bulkFeedback && <Toast
          kind={bulkFeedback.kind}
          message={bulkFeedback.message}
          onClose={() => setBulkFeedback(null)}
          resetKey={bulkFeedbackToken}
        />}
        <label className="browser-auto-refresh">
          <span>键列表自动刷新</span>
          <Select value={refreshSeconds} onChange={(event) => setRefreshSeconds(Number(event.target.value))}>
            <option value={0}>关闭</option>
            <option value={2}>每 2 秒</option>
            <option value={5}>每 5 秒</option>
            <option value={10}>每 10 秒</option>
            <option value={30}>每 30 秒</option>
          </Select>
        </label>
        {refreshSeconds > 0 && <small>选择键、查看详情或编辑期间暂停自动刷新。</small>}
        <StartBulkDeleteButton connectionId={connectionId} keys={state.selectedKeys} disabled={listBusy} onBusyChange={setBulkStartLoading} />
        <BulkKeyActions
          connectionId={connectionId}
          selectedKeys={state.selectedKeys}
          busy={listBusy}
          onDeleted={handleBulkDeleted}
          onError={handleBulkError}
          onBusyChange={setBulkActionLoading}
        />
        <BrowserImportExport
          connectionId={connectionId}
          selectedKeys={state.selectedKeys}
          onImported={handleImported}
          disabled={listBusy}
          onBusyChange={setFileActionLoading}
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

      <fieldset className="browser-layout" disabled={databaseSwitching} aria-label="键浏览与编辑">
        <KeyList
          key={connectionId}
          pattern={state.pattern}
          keyType={state.keyType}
          keys={visibleKeys}
          selectedKey={state.selectedKey}
          selectedKeys={state.selectedKeys}
          arraySupported={arraySupported}
          vectorSetSupported={vectorSetSupported}
          hasMore={state.hasMore}
          loading={state.loading}
          busy={listBusy}
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
      </fieldset>
    </section>
  );
}

export default BrowserPage;
