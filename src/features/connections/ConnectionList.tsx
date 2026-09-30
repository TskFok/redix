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

function isLoopbackHost(host: string): boolean {
  const normalized = host.trim().toLowerCase();
  if (normalized === "localhost" || normalized === "localhost.") return true;

  const octets = normalized.split(".");
  if (octets.length === 4 && octets[0] === "127"
    && octets.every((octet) => /^\d{1,3}$/.test(octet) && Number(octet) <= 255)) {
    return true;
  }

  if (!normalized.includes(":")) return false;
  try {
    const address = normalized.startsWith("[") ? normalized : `[${normalized}]`;
    return new URL(`http://${address}`).hostname === "[::1]";
  } catch {
    return false;
  }
}

function isLocalConnection(profile: ConnectionProfile): boolean {
  // SSH 中的回环地址属于远端；拓扑连接按配置的全部种子节点判断。
  if (profile.ssh) return false;
  const nodes = profile.cluster?.nodes ?? profile.sentinel?.nodes;
  return nodes
    ? nodes.length > 0 && nodes.every((node) => isLoopbackHost(node.host))
    : isLoopbackHost(profile.host);
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
        <span>数据库</span>
        <span>是否是本地连接</span>
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
            <span className="connection-database" aria-label={`${profile.name} 数据库`}>
              <span className="connection-detail-label" aria-hidden="true">数据库：</span>
              DB {profile.cluster ? 0 : profile.database}
            </span>
            <span className="connection-local" aria-label={`${profile.name} 是否是本地连接`}>
              <span className="connection-detail-label" aria-hidden="true">是否是本地连接：</span>
              {isLocalConnection(profile) ? "是" : "否"}
            </span>
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
