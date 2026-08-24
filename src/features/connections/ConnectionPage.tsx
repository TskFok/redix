import { useEffect, useState, type ChangeEvent } from "react";

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
  onOpenConnection: (profile: ConnectionProfile | null) => void;
}

interface TransferFeedback {
  kind: "success" | "warning";
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
    kind: result.failed.length > 0 ? "warning" : "success",
    message:
      result.failed.length > 0
        ? `部分导入：成功 ${result.imported.length} 条，失败 ${result.failed.length} 条。`
        : `连接已导入：${result.imported.length} 条。`,
    details,
  };
}

export function ConnectionPage({ onOpenConnection }: ConnectionPageProps) {
  const [state, setState] = useState<ConnectionPageState>(() => ({
    ...initialConnectionPageState,
  }));
  const [formOpen, setFormOpen] = useState(false);
  const [transferBusy, setTransferBusy] = useState(false);
  const [transferFeedback, setTransferFeedback] = useState<TransferFeedback | null>(null);

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

  const handleOpenFailed = (profile: ConnectionProfile) => {
    setState((current) => ({
      ...current,
      profiles: replaceProfile(current.profiles, profile),
      error: savedButOpenFailedMessage,
    }));
  };

  const handleOpen = async (profile: ConnectionProfile) => {
    const previousActiveId = state.activeId;
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
        error: toUserFacingError(caught, "打开连接失败，请稍后重试。"),
      }));
    } finally {
      setState((current) => ({ ...current, openingId: null }));
    }
  };

  const handleDelete = async (profile: ConnectionProfile) => {
    if (!window.confirm(`确定删除连接“${profile.name}”吗？`)) {
      return;
    }
    const wasEditing = state.editingProfile?.id === profile.id;
    const wasActive = state.activeId === profile.id;

    setState((current) => ({
      ...current,
      deletingId: profile.id,
      error: null,
    }));
    try {
      await deleteConnection(profile.id);
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
      setState((current) => ({
        ...current,
        deletingId: null,
        error: toUserFacingError(caught, "删除连接失败，请稍后重试。"),
      }));
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
      setTransferFeedback(transferFeedbackFromResult(result));
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

  return (
    <section
      className="connections-page"
      aria-labelledby={formOpen ? "connection-form-title" : "connections-page-title"}
      aria-busy={
        state.loading || state.saving || Boolean(state.testingId) || transferBusy
      }
    >
      {!formOpen ? (
        <>
          <div className="page-heading">
            <div>
              <p className="eyebrow">数据连接</p>
              <h2 id="connections-page-title">连接管理</h2>
              <p className="page-description">管理本地 Redis 实例，安全地保存连接配置。</p>
            </div>
            <div className="page-heading-actions">
              <label className="button button-secondary transfer-file-button">
                导入连接
                <input
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

          {state.error ? (
            <p className="feedback feedback-error" role="alert">
              {state.error}
            </p>
          ) : null}

          {transferFeedback ? (
            <div
              className={`transfer-feedback transfer-feedback-${transferFeedback.kind}`}
              role="status"
              aria-live="polite"
            >
              <strong>{transferFeedback.message}</strong>
              {transferFeedback.details.length > 0 ? (
                <ul>
                  {transferFeedback.details.map((detail) => (
                    <li key={detail}>{detail}</li>
                  ))}
                </ul>
              ) : null}
            </div>
          ) : null}

          {state.loading ? (
            <p className="loading-state" role="status" aria-live="polite">
              正在加载连接…
            </p>
          ) : (
            <ConnectionList
              profiles={state.profiles}
              activeId={state.activeId}
              openingId={state.openingId}
              deletingId={state.deletingId}
              onAdd={handleAdd}
              onEdit={handleEdit}
              onOpen={(profile) => void handleOpen(profile)}
              onDelete={(profile) => void handleDelete(profile)}
            />
          )}
        </>
      ) : (
        <ConnectionForm
          initial={state.editingProfile ?? undefined}
          onSaved={handleSaved}
          onCancel={handleCancel}
          onOpened={handleOpened}
          onOpenFailed={handleOpenFailed}
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
