import { useEffect, useState } from "react";

import {
  closeConnection,
  deleteConnection,
  listConnections,
  openConnection,
} from "../../lib/tauri";
import type { ConnectionProfile } from "../../lib/types";
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

export function ConnectionPage({ onOpenConnection }: ConnectionPageProps) {
  const [state, setState] = useState<ConnectionPageState>(() => ({
    ...initialConnectionPageState,
  }));
  const [formOpen, setFormOpen] = useState(false);

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

  const editingId = state.editingProfile?.id ?? "new";

  return (
    <section
      className="connections-page"
      aria-labelledby={formOpen ? "connection-form-title" : "connections-page-title"}
      aria-busy={state.loading || state.saving || Boolean(state.testingId)}
    >
      {!formOpen ? (
        <>
          <div className="page-heading">
            <div>
              <p className="eyebrow">数据连接</p>
              <h2 id="connections-page-title">连接管理</h2>
              <p className="page-description">管理本地 Redis 实例，安全地保存连接配置。</p>
            </div>
            {!state.loading && state.profiles.length > 0 ? (
              <button type="button" className="button button-primary" onClick={handleAdd}>
                新增连接
              </button>
            ) : null}
          </div>

          {state.error ? (
            <p className="feedback feedback-error" role="alert">
              {state.error}
            </p>
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
