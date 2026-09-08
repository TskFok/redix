import { useEffect, useMemo, useRef, useState } from "react";

import {
  deleteQueryLibraryItem,
  listQueryLibrary,
  saveQueryLibraryItem,
} from "../../lib/tauri";
import type { QueryLibraryItem, QueryLibraryItemInput } from "../../lib/types";
import QueryPackage from "./QueryPackage";
import {
  filterQueryLibraryItems,
  initialQueryLibraryPageState,
  normalizeQueryLibraryError,
  normalizeTags,
  type QueryLibraryPageState,
} from "./queryLibraryState";

interface QueryLibraryPageProps {
  onFill(command: string): void;
}

const emptyDraft: QueryLibraryItemInput = {
  id: null,
  name: "",
  command: "",
  tags: [],
};

function draftFromItem(item: QueryLibraryItem): QueryLibraryItemInput {
  return {
    id: item.id,
    name: item.name,
    command: item.command,
    tags: item.tags,
  };
}

export function QueryLibraryPage({ onFill }: QueryLibraryPageProps) {
  const [state, setState] = useState<QueryLibraryPageState>(() => ({
    ...initialQueryLibraryPageState,
  }));
  const [draft, setDraft] = useState<QueryLibraryItemInput | null>(null);
  const mountedRef = useRef(false);
  const requestRef = useRef(0);
  const visibleItems = useMemo(
    () => filterQueryLibraryItems(state.items, state.query),
    [state.items, state.query],
  );

  useEffect(() => {
    mountedRef.current = true;
    const requestId = requestRef.current + 1;
    requestRef.current = requestId;
    setState((current) => ({ ...current, loading: true, error: null }));

    void listQueryLibrary()
      .then((items) => {
        if (mountedRef.current && requestRef.current === requestId) {
          setState((current) => ({ ...current, items, loading: false }));
        }
      })
      .catch((caught) => {
        if (mountedRef.current && requestRef.current === requestId) {
          setState((current) => ({
            ...current,
            loading: false,
            error: normalizeQueryLibraryError(caught),
          }));
        }
      });

    return () => {
      mountedRef.current = false;
      requestRef.current += 1;
    };
  }, []);

  const startNew = () => {
    setDraft({ ...emptyDraft, tags: [] });
    setState((current) => ({ ...current, error: null }));
  };

  const startEdit = (item: QueryLibraryItem) => {
    setDraft(draftFromItem(item));
    setState((current) => ({ ...current, error: null }));
  };

  const handleSave = async () => {
    if (!draft) {
      return;
    }
    const input: QueryLibraryItemInput = {
      ...draft,
      name: draft.name.trim(),
      command: draft.command.trim(),
      tags: draft.tags.map((tag) => tag.trim()).filter((tag) => tag.length > 0),
    };
    if (!input.name || !input.command) {
      setState((current) => ({
        ...current,
        error: normalizeQueryLibraryError({ code: "INVALID_CONNECTION" }),
      }));
      return;
    }

    setState((current) => ({ ...current, saving: true, error: null }));
    try {
      const saved = await saveQueryLibraryItem(input);
      if (!mountedRef.current) {
        return;
      }
      setState((current) => {
        const exists = current.items.some((item) => item.id === saved.id);
        return {
          ...current,
          items: exists
            ? current.items.map((item) => (item.id === saved.id ? saved : item))
            : [saved, ...current.items],
          saving: false,
          error: null,
        };
      });
      setDraft(null);
    } catch (caught) {
      if (mountedRef.current) {
        setState((current) => ({
          ...current,
          saving: false,
          error: normalizeQueryLibraryError(caught),
        }));
      }
    }
  };

  const handleDelete = async (item: QueryLibraryItem) => {
    if (!window.confirm(`确认删除查询“${item.name}”？`)) {
      return;
    }
    try {
      await deleteQueryLibraryItem(item.id);
      if (mountedRef.current) {
        setState((current) => ({
          ...current,
          items: current.items.filter((currentItem) => currentItem.id !== item.id),
          error: null,
        }));
      }
    } catch (caught) {
      if (mountedRef.current) {
        setState((current) => ({
          ...current,
          error: normalizeQueryLibraryError(caught),
        }));
      }
    }
  };

  return (
    <section className="query-library-page" aria-labelledby="query-library-page-title">
      <div className="page-heading">
        <div>
          <p className="eyebrow">QUERY LIBRARY</p>
          <h2 id="query-library-page-title">Query Library</h2>
          <p className="page-description">保存常用 Redis 命令，随时回填到 Workbench。</p>
        </div>
        <button type="button" className="button button-primary" onClick={startNew}>
          新建查询
        </button>
      </div>

      <QueryPackage disabled={state.loading || state.saving} onImported={(items) => setState((current) => ({ ...current, items: [...items, ...current.items] }))} />

      {state.error ? (
        <p className="feedback feedback-error" role="alert">
          {state.error.message}
        </p>
      ) : null}

      <label className="field query-library-search">
        <span>搜索</span>
        <input
          autoCapitalize="off"
          autoCorrect="off"
          aria-label="搜索已保存查询"
          value={state.query}
          placeholder="按名称、命令或标签搜索"
          onChange={(event) =>
            setState((current) => ({ ...current, query: event.target.value }))
          }
        />
      </label>

      {state.loading ? (
        <p className="loading-state" role="status">
          加载已保存查询…
        </p>
      ) : visibleItems.length > 0 ? (
        <div className="query-library-list" aria-label="已保存查询">
          {visibleItems.map((item) => (
            <article className="query-library-card" key={item.id}>
              <div className="query-library-card-heading">
                <div>
                  <h3>{item.name}</h3>
                  <code>{item.command}</code>
                </div>
                <time dateTime={new Date(item.updated_at).toISOString()}>
                  {new Date(item.updated_at).toLocaleDateString("zh-CN")}
                </time>
              </div>
              {item.tags.length > 0 ? (
                <div className="query-library-tags" aria-label={`${item.name} 标签`}>
                  {item.tags.map((tag) => (
                    <span key={tag}>{tag}</span>
                  ))}
                </div>
              ) : null}
              <div className="card-actions">
                <button
                  type="button"
                  className="button button-primary"
                  onClick={() => onFill(item.command)}
                >
                  回填 Workbench {item.name}
                </button>
                <button
                  type="button"
                  className="button button-secondary"
                  onClick={() => startEdit(item)}
                >
                  编辑查询 {item.name}
                </button>
                <button
                  type="button"
                  className="button button-danger"
                  onClick={() => void handleDelete(item)}
                >
                  删除查询 {item.name}
                </button>
              </div>
            </article>
          ))}
        </div>
      ) : (
        <div className="empty-state">
          <p>{state.query ? "没有匹配的已保存查询" : "还没有已保存查询"}</p>
          <button type="button" className="button button-secondary" onClick={startNew}>
            新建查询
          </button>
        </div>
      )}

      {draft ? (
        <section className="query-library-editor" aria-labelledby="query-library-editor-title">
          <div className="panel-heading">
            <div>
              <p className="eyebrow">SAVE QUERY</p>
              <h2 id="query-library-editor-title">
                {draft.id ? "编辑查询" : "新建查询"}
              </h2>
            </div>
          </div>
          <label className="field">
            <span>查询名称</span>
            <input
              autoCapitalize="off"
              autoCorrect="off"
              value={draft.name}
              onChange={(event) =>
                setDraft((current) => (current ? { ...current, name: event.target.value } : current))
              }
            />
          </label>
          <label className="field">
            <span>Redis 命令</span>
            <textarea
              autoCapitalize="off"
              autoCorrect="off"
              value={draft.command}
              rows={4}
              onChange={(event) =>
                setDraft((current) =>
                  current ? { ...current, command: event.target.value } : current,
                )
              }
            />
          </label>
          <label className="field">
            <span>标签</span>
            <input
              autoCapitalize="off"
              autoCorrect="off"
              value={draft.tags.join(", ")}
              placeholder="例如：用户, 读取"
              onChange={(event) =>
                setDraft((current) =>
                  current ? { ...current, tags: normalizeTags(event.target.value) } : current,
                )
              }
            />
          </label>
          <div className="form-actions">
            <button
              type="button"
              className="button button-primary"
              disabled={state.saving}
              onClick={() => void handleSave()}
            >
              保存查询
            </button>
            <button
              type="button"
              className="button button-secondary"
              disabled={state.saving}
              onClick={() => setDraft(null)}
            >
              取消
            </button>
          </div>
        </section>
      ) : null}
    </section>
  );
}

export default QueryLibraryPage;
