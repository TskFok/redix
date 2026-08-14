import { useCallback, useEffect, useRef, useState } from "react";

import { getKey, scanKeys } from "../../lib/tauri";
import type { KeyValue } from "../../lib/types";
import KeyDetails from "./KeyDetails";
import KeyList from "./KeyList";
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

  const handleDeleted = (key: string) => {
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

      <div className="browser-layout">
        <KeyList
          pattern={state.pattern}
          keys={state.keys}
          selectedKey={state.selectedKey}
          hasMore={state.hasMore}
          loading={listBusy}
          onPatternChange={handlePatternChange}
          onPatternKeyDown={handlePatternKeyDown}
          onSelect={(key) => void handleSelect(key)}
          onLoadMore={handleLoadMore}
        />
        <KeyDetails
          connectionId={connectionId}
          detail={state.detail}
          loading={detailLoading}
          onDetailChange={handleDetailChange}
          onDeleted={handleDeleted}
          onBusyChange={setDetailActionLoading}
        />
      </div>
    </section>
  );
}

export default BrowserPage;
