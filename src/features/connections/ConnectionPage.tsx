import { useEffect, useRef, useState, type ChangeEvent } from "react";
import { useConfirmDialog } from "../../components/useConfirmDialog";
import Toast from "../../components/Toast";
import { useTransientFeedback } from "../../components/useTransientFeedback";

import {
  closeConnection,
  deleteConnection,
  exportConnections,
  importConnections,
  listConnections,
  openConnection,
} from "../../lib/tauri";
import type {
  ConnectionExportDocument,
  ConnectionProfile,
  ImportConnectionsResult,
} from "../../lib/types";
import ConnectionForm from "./ConnectionForm";
import ConnectionList from "./ConnectionList";
import { listConnectionTags, type ConnectionTags } from "../../lib/localProductsApi";
import { filterConnectionsByTag } from "./ConnectionTags";
import {
  initialConnectionPageState,
  connectionCleanupFailedMessage,
  connectionSwitchFailedMessage,
  replaceProfile,
  savedButOpenFailedMessage,
  toUserFacingError,
  type ConnectionPageState,
} from "./connectionState";

interface ConnectionPageProps {
  activeConnectionId?: string | null;
  onOpenConnection: (profile: ConnectionProfile | null) => void;
}

interface TransferFeedback {
  kind: "success" | "error";
  message: string;
  details: string[];
}

const MAX_IMPORT_BYTES = 10 * 1024 * 1024;

function downloadConnectionDocument(documentValue: ConnectionExportDocument) {
  const payload = JSON.stringify(documentValue, null, 2);
  const blob = new Blob([payload], { type: "application/json" });
  const objectUrl =
    typeof URL.createObjectURL === "function"
      ? URL.createObjectURL(blob)
      : `data:application/json;charset=utf-8,${encodeURIComponent(payload)}`;
  const link = document.createElement("a");
  link.href = objectUrl;
  link.download = "redix-connections.json";
  link.setAttribute("aria-hidden", "true");
  try {
    link.click();
  } finally {
    if (objectUrl.startsWith("blob:") && typeof URL.revokeObjectURL === "function") {
      URL.revokeObjectURL(objectUrl);
    }
  }
}

function transferFeedbackFromResult(result: ImportConnectionsResult): TransferFeedback {
  const details = result.failed.map((failure) =>
    `${failure.name ?? `第 ${failure.index + 1} 条`}: ${failure.message}`,
  );
  if (result.ignored_secret_fields > 0) {
    details.push(`已忽略 ${result.ignored_secret_fields} 个敏感字段，需在本机重新录入。`);
  }
  return {
    kind: result.failed.length > 0 || result.ignored_secret_fields > 0 ? "error" : "success",
    message:
      result.failed.length > 0
        ? `部分导入：成功 ${result.imported.length} 条，失败 ${result.failed.length} 条。`
        : `连接已导入：${result.imported.length} 条。`,
    details,
  };
}

export function ConnectionPage({ activeConnectionId, onOpenConnection }: ConnectionPageProps) {
  const [state, setState] = useState<ConnectionPageState>(() => ({
    ...initialConnectionPageState,
  }));
  const [formOpen, setFormOpen] = useState(false);
  const [transferBusy, setTransferBusy] = useState(false);
  const [transferFeedback, setTransferFeedback, transferFeedbackToken] = useTransientFeedback<TransferFeedback>();
  const [tags, setTags] = useState<ConnectionTags | null>(null);
  const [tagsError, setTagsError] = useState(false);
  const [tagQuery, setTagQuery] = useState("");
  const [onlyUntagged, setOnlyUntagged] = useState(false);
  const activeId = activeConnectionId === undefined ? state.activeId : activeConnectionId;
  const { confirm, confirmationDialog } = useConfirmDialog(JSON.stringify([
    activeId, state.profiles, state.editingProfile, formOpen, state.loading, state.saving,
    state.testingId, state.openingId, state.deletingId, transferBusy, tags, tagQuery, onlyUntagged,
  ]));
  const currentConfirmRef = useRef(confirm);
  currentConfirmRef.current = confirm;
  const deleteInFlight = useRef(false);
  const mountedRef = useRef(false);

  useEffect(() => {
    mountedRef.current = true;
    return () => { mountedRef.current = false; };
  }, []);

  useEffect(() => {
    let mounted = true;
    void listConnectionTags().then((loaded) => { if (mounted) { setTags(loaded); setTagsError(false); } })
      .catch(() => { if (mounted) setTagsError(true); });
    return () => { mounted = false; };
  }, []);

  useEffect(() => {
    let mounted = true;
    void listConnections()
      .then((profiles) => {
        if (!mounted) {
          return;
        }
        setState((current) => ({
          ...current,
          profiles,
          loading: false,
          error: null,
          activeId: profiles.some((profile) => profile.id === current.activeId)
            ? current.activeId
            : null,
        }));
      })
      .catch((caught: unknown) => {
        if (!mounted) {
          return;
        }
        setState((current) => ({
          ...current,
          loading: false,
          error: toUserFacingError(caught, "加载连接列表失败，请稍后重试。"),
        }));
      });

    return () => {
      mounted = false;
    };
  }, []);

  const handleSaved = (profile: ConnectionProfile) => {
    setState((current) => ({
      ...current,
      profiles: replaceProfile(current.profiles, profile),
      editingProfile: null,
      error: null,
    }));
    setFormOpen(false);
  };

  const handleOpened = (profile: ConnectionProfile) => {
    setState((current) => ({
      ...current,
      activeId: profile.id,
      error: null,
    }));
    onOpenConnection(profile);
  };

  const handleOpen = async (profile: ConnectionProfile, openFailedMessage?: string) => {
    const previousActiveId = activeId;
    setState((current) => ({
      ...current,
      openingId: profile.id,
      error: null,
    }));
    try {
      await openConnection(profile.id);
      if (previousActiveId && previousActiveId !== profile.id) {
        try {
          await closeConnection(previousActiveId);
        } catch {
          let cleanupFailed = false;
          try {
            await closeConnection(profile.id);
          } catch {
            cleanupFailed = true;
          }
          setState((current) => ({
            ...current,
            activeId: previousActiveId,
            error: cleanupFailed
              ? connectionCleanupFailedMessage
              : connectionSwitchFailedMessage,
          }));
          return;
        }
      }
      handleOpened(profile);
    } catch (caught) {
      setState((current) => ({
        ...current,
        error: openFailedMessage ?? toUserFacingError(caught, "打开连接失败，请稍后重试。"),
      }));
    } finally {
      setState((current) => ({ ...current, openingId: null }));
    }
  };

  const handleDelete = async (profile: ConnectionProfile) => {
    if (deleteInFlight.current || state.loading || state.saving || state.testingId || state.openingId || transferBusy) {
      return;
    }
    if (!await confirm(`确定删除连接“${profile.name}”吗？`)) return;
    if (!mountedRef.current || currentConfirmRef.current !== confirm || deleteInFlight.current) return;
    const wasEditing = state.editingProfile?.id === profile.id;
    const wasActive = activeId === profile.id;
    deleteInFlight.current = true;

    setState((current) => ({
      ...current,
      deletingId: profile.id,
      error: null,
    }));
    try {
      await deleteConnection(profile.id);
      if (!mountedRef.current) return;
      setState((current) => ({
        ...current,
        profiles: current.profiles.filter((item) => item.id !== profile.id),
        activeId: current.activeId === profile.id ? null : current.activeId,
        editingProfile:
          current.editingProfile?.id === profile.id ? null : current.editingProfile,
        deletingId: null,
      }));
      if (wasEditing) {
        setFormOpen(false);
      }
      if (wasActive) {
        onOpenConnection(null);
      }
    } catch (caught) {
      if (!mountedRef.current) return;
      setState((current) => ({
        ...current,
        deletingId: null,
        error: toUserFacingError(caught, "删除连接失败，请稍后重试。"),
      }));
    } finally {
      deleteInFlight.current = false;
    }
  };

  const handleAdd = () => {
    setState((current) => ({
      ...current,
      editingProfile: null,
      error: null,
    }));
    setFormOpen(true);
  };

  const handleEdit = (profile: ConnectionProfile) => {
    setState((current) => ({
      ...current,
      editingProfile: profile,
      error: null,
    }));
    setFormOpen(true);
  };

  const handleCancel = () => {
    setState((current) => ({
      ...current,
      editingProfile: null,
      error: null,
      testingId: null,
      saving: false,
    }));
    setFormOpen(false);
  };

  const handleExport = async () => {
    if (transferBusy) {
      return;
    }
    setTransferBusy(true);
    setTransferFeedback(null);
    setState((current) => ({ ...current, error: null }));
    try {
      const documentValue = await exportConnections();
      downloadConnectionDocument(documentValue);
      setTransferFeedback({
        kind: "success",
        message: `连接已导出：${documentValue.connections.length} 条。`,
        details: ["导出文件不包含密码、CA PEM、客户端证书或私钥。"],
      });
    } catch (caught) {
      setState((current) => ({
        ...current,
        error: toUserFacingError(caught, "导出连接失败，请稍后重试。"),
      }));
    } finally {
      setTransferBusy(false);
    }
  };

  const handleImportFile = async (event: ChangeEvent<HTMLInputElement>) => {
    const file = event.target.files?.[0];
    event.target.value = "";
    if (!file || transferBusy) {
      return;
    }
    if (file.size > MAX_IMPORT_BYTES) {
      setTransferFeedback(null);
      setState((current) => ({
        ...current,
        error: "连接导入文件不能超过 10 MB。",
      }));
      return;
    }

    setTransferBusy(true);
    setTransferFeedback(null);
    setState((current) => ({ ...current, error: null }));
    try {
      const result = await importConnections({ content: await file.text() });
      if (result.imported.length > 0) {
        setState((current) => ({
          ...current,
          profiles: [...current.profiles, ...result.imported],
          error: null,
        }));
      }
      const feedback = transferFeedbackFromResult(result);
      setTransferFeedback(feedback);
    } catch (caught) {
      setState((current) => ({
        ...current,
        error: toUserFacingError(caught, "导入连接失败，请检查 JSON 文件。"),
      }));
    } finally {
      setTransferBusy(false);
    }
  };

  const editingId = state.editingProfile?.id ?? "new";
  const visibleProfiles = tags ? filterConnectionsByTag(state.profiles, tags, tagQuery, onlyUntagged) : state.profiles;
  const hasFilters = Boolean(tagQuery.trim()) || onlyUntagged;

  const clearFilters = () => {
    setTagQuery("");
    setOnlyUntagged(false);
  };

  return (
    <section
      className="connections-page"
      aria-labelledby={formOpen ? "connection-form-title" : "connections-page-title"}
      aria-busy={
        state.loading || state.saving || Boolean(state.testingId) || transferBusy
      }
    >
      {confirmationDialog}
      {!formOpen ? (
        <>
          <div className="page-heading connections-heading">
            <div>
              <p className="eyebrow">数据连接</p>
              <h2 id="connections-page-title">连接管理</h2>
              <p className="page-description">管理本地 Redis 实例，安全地保存连接配置。</p>
            </div>
            <div className="page-heading-actions">
              <label className="button button-secondary transfer-file-button">
                导入连接
                <input
                  autoCapitalize="off"
                  autoCorrect="off"
                  type="file"
                  aria-label="导入连接文件"
                  accept="application/json,.json"
                  onChange={(event) => void handleImportFile(event)}
                  disabled={state.loading || transferBusy}
                />
              </label>
              <button
                type="button"
                className="button button-secondary"
                onClick={() => void handleExport()}
                disabled={state.loading || transferBusy}
              >
                {transferBusy ? "处理中…" : "导出连接"}
              </button>
              {!state.loading && state.profiles.length > 0 ? (
                <button type="button" className="button button-primary" onClick={handleAdd}>
                  新增连接
                </button>
              ) : null}
            </div>
          </div>

          {state.error ? <Toast kind="error" message={state.error} resetKey={state} onClose={() => setState((current) => ({ ...current, error: null }))} /> : null}

          {transferFeedback ? <Toast kind={transferFeedback.kind} message={transferFeedback.message} details={transferFeedback.details} resetKey={transferFeedbackToken} onClose={() => setTransferFeedback(null)} /> : null}

          {state.loading ? (
            <p className="loading-state" role="status" aria-live="polite">
              正在加载连接…
            </p>
          ) : (
            <>
              {state.profiles.length > 0 && (
                <div className="connections-toolbar" role="group" aria-label="连接筛选">
                  {tags && (
                    <div className="connections-filters">
                      <label className="field connections-filter-search">
                        <span>筛选连接标签</span>
                        <input
                          autoCapitalize="off"
                          autoCorrect="off"
                          value={tagQuery}
                          placeholder="按标签键或值搜索，例如 env=prod"
                          onChange={(event) => setTagQuery(event.target.value)}
                        />
                      </label>
                      <label className="checkbox-field connections-filter-toggle">
                        <input
                          type="checkbox"
                          checked={onlyUntagged}
                          onChange={(event) => setOnlyUntagged(event.target.checked)}
                        />
                        <span>仅显示无标签连接</span>
                      </label>
                      {hasFilters && (
                        <button type="button" className="button button-quiet" onClick={clearFilters}>
                          清除筛选
                        </button>
                      )}
                    </div>
                  )}
                  <p className="connections-count" aria-live="polite" aria-atomic="true">
                    {hasFilters && tags
                      ? `显示 ${visibleProfiles.length} / ${state.profiles.length} 个连接`
                      : `共 ${state.profiles.length} 个连接`}
                  </p>
                </div>
              )}
              {tagsError && (
                <div className="connections-tags-error" role="status">
                  <span>连接标签加载失败。</span>
                  <button
                    type="button"
                    className="button button-quiet"
                    onClick={() => {
                      void listConnectionTags().then((loaded) => {
                        setTags(loaded);
                        setTagsError(false);
                      }).catch(() => setTagsError(true));
                    }}
                  >
                    重试加载标签
                  </button>
                </div>
              )}
              {state.profiles.length > 0 && visibleProfiles.length === 0 ? (
                <div className="empty-state connections-empty-filter">
                  <h3>没有匹配标签的连接。</h3>
                  <p>试试其他标签，或清除筛选查看全部连接。</p>
                </div>
              ) : (
                <ConnectionList
                  profiles={visibleProfiles}
                  tags={tags ?? undefined}
                  onTagsSaved={(id, updated) => setTags((current) => ({ ...current, [id]: updated }))}
                  activeId={activeId}
                  openingId={state.openingId}
                  deletingId={state.deletingId}
                  onAdd={handleAdd}
                  onEdit={handleEdit}
                  onOpen={(profile) => void handleOpen(profile)}
                  onDelete={(profile) => void handleDelete(profile)}
                />
              )}
            </>
          )}
        </>
      ) : (
        <ConnectionForm
          initial={state.editingProfile ?? undefined}
          onSaved={handleSaved}
          onCancel={handleCancel}
          onConnect={(profile) => handleOpen(profile, savedButOpenFailedMessage)}
          onTestingChange={(testing) =>
            setState((current) => ({
              ...current,
              testingId: testing ? editingId : null,
            }))
          }
          onSavingChange={(saving) =>
            setState((current) => ({ ...current, saving }))
          }
        />
      )}
    </section>
  );
}

export default ConnectionPage;
