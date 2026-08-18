import { useCallback, useEffect, useRef, useState } from "react";

import { getKey, scanKeys } from "../../lib/tauri";
import type { KeyValue } from "../../lib/types";
import KeyDetails from "./KeyDetails";
import KeyList from "./KeyList";
import AddKey from "./AddKey";
import BulkKeyActions from "./BulkKeyActions";
import {
  applyScanPage,
  browserErrorMessage,
  initialBrowserPageState,
  type BrowserPageState,
} from "./browserState";

interface BrowserPageProps {
  connectionId: string;
}

const SCAN_COUNT = 100;
const FILTER_DEBOUNCE_MS = 320;

export function BrowserPage({ connectionId }: BrowserPageProps) {
  const [state, setState] = useState<BrowserPageState>(() => ({
    ...initialBrowserPageState,
  }));
  const [detailLoading, setDetailLoading] = useState(false);
  const [detailActionLoading, setDetailActionLoading] = useState(false);
  const [selectedKeys, setSelectedKeys] = useState<string[]>([]);
  const [showAddKey, setShowAddKey] = useState(false);
  const connectionIdRef = useRef(connectionId);
  connectionIdRef.current = connectionId;
  const mountedRef = useRef(false);
  const scanLoadingRef = useRef(false);
  const scanRequestRef = useRef(0);
  const detailRequestRef = useRef(0);
  const debounceRef = useRef<number | null>(null);
  const skipDebounceForPatternRef = useRef<string | null>(null);

  const scanPage = useCallback(
    async (cursor: number, requestedPattern: string, replace: boolean) => {
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
        cursor: replace ? 0 : current.cursor,
        keys: replace ? [] : current.keys,
        selectedKey: replace ? null : current.selectedKey,
        detail: replace ? null : current.detail,
        loading: true,
        error: null,
      }));
      if (replace) {
        setSelectedKeys([]);
      }

      try {
        const page = await scanKeys({
          connection_id: connectionId,
          cursor,
          pattern: requestedPattern,
          count: SCAN_COUNT,
        });
        if (mountedRef.current && scanRequestRef.current === requestId) {
          setState((current) => applyScanPage(current, page, replace));
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
    [connectionId],
  );

  useEffect(() => {
    mountedRef.current = true;
    scanLoadingRef.current = false;
    skipDebounceForPatternRef.current = "*";
    setDetailActionLoading(false);
    setSelectedKeys([]);
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
      detailRequestRef.current += 1;
      if (debounceRef.current !== null) {
        window.clearTimeout(debounceRef.current);
        debounceRef.current = null;
      }
    };
  }, [connectionId, scanPage]);

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
    setSelectedKeys([]);
    void scanPage(0, state.pattern.trim() || "*", true);
  };

  const handleToggleSelect = (key: string) => {
    if (state.loading || detailActionLoading) {
      return;
    }
    setSelectedKeys((current) =>
      current.includes(key) ? current.filter((item) => item !== key) : [...current, key],
    );
  };

  const handleCreated = () => {
    setShowAddKey(false);
    setSelectedKeys([]);
    void scanPage(0, state.pattern.trim() || "*", true);
  };

  const handleBulkDeleted = () => {
    setSelectedKeys([]);
    void scanPage(0, state.pattern.trim() || "*", true);
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
      error: null,
    }));

    try {
      const detail = await getKey({ connection_id: connectionId, key });
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
      detail: current.detail?.key === key ? null : current.detail,
      error: null,
    }));
  };

  const listBusy = state.loading || detailLoading || detailActionLoading;

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
        <BulkKeyActions
          connectionId={connectionId}
          selectedKeys={selectedKeys}
          busy={listBusy}
          onDeleted={handleBulkDeleted}
          onError={handleBulkError}
        />
      </div>

      {showAddKey ? (
        <AddKey
          connectionId={connectionId}
          busy={listBusy}
          onCreated={handleCreated}
          onCancel={() => setShowAddKey(false)}
        />
      ) : null}

      <div className="browser-layout">
        <KeyList
          pattern={state.pattern}
          keys={state.keys}
          selectedKey={state.selectedKey}
          selectedKeys={selectedKeys}
          hasMore={state.hasMore}
          loading={listBusy}
          onPatternChange={handlePatternChange}
          onPatternKeyDown={handlePatternKeyDown}
          onSelect={(key) => void handleSelect(key)}
          onToggleSelect={handleToggleSelect}
          onLoadMore={handleLoadMore}
        />
        <KeyDetails
          connectionId={connectionId}
          detail={state.detail}
          loading={detailLoading}
          onDetailChange={handleDetailChange}
          onRenamed={handleRenamed}
          onDeleted={handleDeleted}
          onBusyChange={setDetailActionLoading}
        />
      </div>
    </section>
  );
}

export default BrowserPage;
