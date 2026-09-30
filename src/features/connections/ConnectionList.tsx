import type { ConnectionProfile } from "../../lib/types";

interface ConnectionListProps {
  profiles: ConnectionProfile[];
  activeId: string | null;
  openingId: string | null;
  deletingId: string | null;
  onEdit: (profile: ConnectionProfile) => void;
  onOpen: (profile: ConnectionProfile) => void;
  onDelete: (profile: ConnectionProfile) => void;
}

export function ConnectionList({
  profiles,
  activeId,
  openingId,
  deletingId,
  onEdit,
  onOpen,
  onDelete,
}: ConnectionListProps) {
  if (profiles.length === 0) {
    return (
      <div className="empty-state">
        <div className="empty-state-mark" aria-hidden="true">
          R
        </div>
        <h2>还没有 Redis 连接</h2>
        <p>添加一个本地 Redis 连接，开始浏览键和值。</p>
      </div>
    );
  }

  return (
    <div className="connection-list" aria-label="Redis 连接列表">
      <div className="connection-list-header" aria-hidden="true">
        <span>连接名称</span>
        <span>连接状态</span>
        <span>操作</span>
      </div>
      {profiles.map((profile) => {
        const isActive = activeId === profile.id;
        const isOpening = openingId === profile.id;
        const isDeleting = deletingId === profile.id;
        return (
          <article
            className={`connection-row${isActive ? " connection-row-active" : ""}`}
            key={profile.id}
          >
            <h3 className="connection-name" title={profile.name}>{profile.name}</h3>
            <span
              className={`connection-status${isOpening ? " status-opening" : isActive ? " status-active" : ""}`}
              role="status"
              aria-label={`${profile.name} 连接状态`}
            >
              <span className="status-dot" aria-hidden="true" />
              {isOpening ? "连接中…" : isActive ? "已连接" : "未连接"}
            </span>
            <div className="connection-row-actions">
              <button
                type="button"
                className="button button-primary button-compact"
                onClick={() => onOpen(profile)}
                disabled={isOpening || isDeleting}
              >
                {isOpening ? "连接中…" : isActive ? "重新连接" : "连接"}
              </button>
              <button
                type="button"
                className="button button-secondary button-compact"
                aria-label={`编辑 ${profile.name}`}
                onClick={() => onEdit(profile)}
                disabled={isOpening || isDeleting}
              >
                编辑
              </button>
              <button
                type="button"
                className="button button-danger button-compact connection-delete-button"
                aria-label={isDeleting ? "删除中…" : `删除 ${profile.name}`}
                onClick={() => onDelete(profile)}
                disabled={isOpening || isDeleting}
              >
                {isDeleting ? "删除中…" : "删除"}
              </button>
            </div>
          </article>
        );
      })}
    </div>
  );
}

export default ConnectionList;
