import type { ConnectionProfile } from "../../lib/types";
import { connectionAddress } from "./connectionState";
import { connectionTagsFor, ConnectionTagsPanel } from "./ConnectionTags";
import type { ConnectionTag, ConnectionTags } from "../../lib/localProductsApi";

interface ConnectionListProps {
  profiles: ConnectionProfile[];
  activeId: string | null;
  openingId: string | null;
  deletingId: string | null;
  onAdd: () => void;
  onEdit: (profile: ConnectionProfile) => void;
  onOpen: (profile: ConnectionProfile) => void;
  onDelete: (profile: ConnectionProfile) => void;
  tags?: ConnectionTags;
  onTagsSaved?: (id: string, tags: ConnectionTag[]) => void;
}

export function ConnectionList({
  profiles,
  activeId,
  openingId,
  deletingId,
  onAdd,
  onEdit,
  onOpen,
  onDelete,
  tags,
  onTagsSaved,
}: ConnectionListProps) {
  if (profiles.length === 0) {
    return (
      <div className="empty-state">
        <div className="empty-state-mark" aria-hidden="true">
          R
        </div>
        <h2>还没有 Redis 连接</h2>
        <p>添加一个本地 Redis 连接，开始浏览键和值。</p>
        <button type="button" className="button button-primary" onClick={onAdd}>
          新增连接
        </button>
      </div>
    );
  }

  return (
    <div className="connection-list" aria-label="Redis 连接列表">
      {profiles.map((profile) => {
        const isActive = activeId === profile.id;
        const isOpening = openingId === profile.id;
        const isDeleting = deletingId === profile.id;
        return (
          <article
            className={`connection-card${isActive ? " connection-card-active" : ""}`}
            key={profile.id}
          >
            <div className="connection-card-heading">
              <div>
                <h3>{profile.name}</h3>
                <p className="connection-address">
                  <code>
                    {connectionAddress(profile)}
                  </code>
                </p>
                {profile.ssh && <p className="connection-address">SSH · {profile.ssh.host}:{profile.ssh.port}</p>}
              </div>
              <span className={`connection-status${isActive ? " status-active" : ""}`}>
                <span className="status-dot" aria-hidden="true" />
                {isActive ? "已连接" : "未连接"}
              </span>
            </div>

            <dl className="connection-metadata">
              <div>
                <dt>数据库</dt>
                <dd>{profile.database}</dd>
              </div>
              <div>
                <dt>认证</dt>
                <dd>{profile.has_password ? "已配置" : "未配置"}</dd>
              </div>
              <div>
                <dt>TLS</dt>
                <dd>{profile.tls ? "已启用" : "未启用"}</dd>
              </div>
              <div>
                <dt>证书</dt>
                <dd>
                  {profile.has_ca_certificate || profile.has_client_certificate
                    ? "已配置"
                    : profile.ca_certificate_name || profile.client_certificate_name
                      ? "需重新录入"
                      : "未配置"}
                </dd>
              </div>
            </dl>

            {tags && onTagsSaved && <ConnectionTagsPanel connectionId={profile.id} connectionName={profile.name} tags={connectionTagsFor(tags, profile.id)} onSaved={onTagsSaved} />}

            <div className="card-actions">
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
