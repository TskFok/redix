import Select from "../../components/Select";
import { useState } from "react";

import {
  openConnection,
  saveConnection,
  testConnection,
} from "../../lib/tauri";
import type { ClusterConfig, ConnectionProfile, SaveConnectionInput, SentinelConfig, SshConfig } from "../../lib/types";
import {
  formValuesFromProfile,
  parseSeedNodes,
  savedButOpenFailedMessage,
  toUserFacingError,
  type ConnectionFormValues,
} from "./connectionState";

interface ConnectionFormProps {
  initial?: ConnectionProfile;
  onSaved: (profile: ConnectionProfile) => void;
  onCancel: () => void;
  onConnect?: (profile: ConnectionProfile) => Promise<void>;
  onOpened?: (profile: ConnectionProfile) => void;
  onOpenFailed?: (profile: ConnectionProfile) => void;
  onTestingChange?: (testing: boolean) => void;
  onSavingChange?: (saving: boolean) => void;
}

interface FormValidation {
  input?: SaveConnectionInput;
  error?: string;
}

function buildConnectionInput(
  values: ConnectionFormValues,
  profileId: string,
  initial?: ConnectionProfile,
): FormValidation {
  const name = values.name.trim();
  let host = values.host.trim();
  let port = Number(values.port);
  const database = values.topology === "cluster" ? 0 : Number(values.database);
  let cluster: ClusterConfig | null = null;
  if (values.topology === "cluster") {
    const nodes = parseSeedNodes(values.cluster_nodes);
    if (!nodes) return { error: "请填写 1 到 32 个有效且唯一的 Cluster 种子节点；IPv6 使用 [地址]:端口。" };
    cluster = { nodes, read_from_replicas: values.cluster_read_from_replicas };
    ({ host, port } = nodes[0]);
  }
  let ssh: SshConfig | null = null;
  if (values.ssh_enabled) {
    if (cluster) return { error: "Cluster 暂不支持 SSH 隧道，请关闭 SSH 后连接。" };
    const sshPort = Number(values.ssh_port);
    if (!values.ssh_host.trim() || !values.ssh_username.trim() || !Number.isInteger(sshPort) || sshPort < 1 || sshPort > 65535) return { error: "请输入有效的 SSH 主机、端口和用户名。" };
    const saved = !values.clear_ssh_secrets && initial?.ssh?.auth_method === values.ssh_auth_method ? initial.ssh : null;
    if (values.ssh_auth_method === "private_key") {
      const newKey = Boolean(values.ssh_private_key.trim());
      const newIdentity = Boolean(values.ssh_identity_file.trim());
      if (newKey && newIdentity) {
        return { error: "SSH 私钥内容和私钥文件路径只能填写一项。" };
      }
      if ((newKey && saved?.has_identity_file) || (newIdentity && saved?.has_private_key)) {
        return { error: "切换私钥来源前，请勾选清除已保存的 SSH 凭据和路径。此操作也会清除 known_hosts 路径和私钥口令，请按需重新填写。" };
      }
    }
    const hasPassword = values.ssh_auth_method === "password" && (Boolean(values.ssh_password) || Boolean(saved?.has_password));
    const hasKey = values.ssh_auth_method === "private_key" && (Boolean(values.ssh_private_key.trim()) || Boolean(saved?.has_private_key));
    const hasIdentity = values.ssh_auth_method === "private_key" && (Boolean(values.ssh_identity_file.trim()) || Boolean(saved?.has_identity_file));
    if (values.ssh_auth_method === "password" && !hasPassword) return { error: "请输入 SSH 密码。" };
    if (values.ssh_auth_method === "private_key" && !hasKey && !hasIdentity) return { error: "请输入 SSH 私钥内容或私钥文件路径。" };
    ssh = {
      host: values.ssh_host.trim(), port: sshPort, username: values.ssh_username.trim(),
      auth_method: values.ssh_auth_method,
      has_password: hasPassword,
      has_private_key: hasKey,
      has_passphrase: values.ssh_auth_method === "private_key" && (Boolean(values.ssh_passphrase) || Boolean(saved?.has_passphrase)),
      has_identity_file: hasIdentity,
      has_known_hosts_file: Boolean(values.ssh_known_hosts_file.trim()) || Boolean(initial?.ssh?.has_known_hosts_file && !values.clear_ssh_secrets),
    };
  }
  let sentinel: SentinelConfig | null = null;
  if (values.topology === "sentinel") {
    const nodes = values.sentinel_nodes.split(/[\n,]+/).map((line) => line.trim()).filter(Boolean).map((line) => {
      const match = /^(?:\[([^\]]+)\]|([^:\s]+)):(\d+)$/.exec(line);
      return match ? { host: match[1] || match[2], port: Number(match[3]) } : null;
    });
    if (!values.sentinel_master_name.trim()) return { error: "请输入 Sentinel 主节点名称。" };
    if (!nodes.length || nodes.length > 32 || nodes.some((node) => !node || node.port < 1 || node.port > 65535)) {
      return { error: "请填写 1 到 32 个有效 Sentinel 节点，每行 host:port；IPv6 使用 [地址]:端口。" };
    }
    sentinel = { master_name: values.sentinel_master_name.trim(), nodes: nodes.filter((node) => node !== null), username: values.sentinel_username.trim() || null,
      has_password: Boolean(values.sentinel_password) || Boolean(initial?.sentinel?.has_password && !values.clear_sentinel_password), tls: values.sentinel_tls };
  }

  if (!name) {
    return { error: "请输入连接名称。" };
  }
  if (!host) {
    return { error: "请输入主机。" };
  }
  if (!Number.isInteger(port) || port < 1 || port > 65535) {
    return { error: "请输入有效端口。" };
  }
  if (!Number.isInteger(database) || database < 0 || database > 15) {
    return { error: "数据库编号必须是 0 到 15。" };
  }

  const password = values.password.length > 0 ? values.password : null;
  const caCertificate = values.ca_certificate.trim() || null;
  const clientCertificate = values.client_certificate.trim() || null;
  const clientKey = values.client_key.trim() || null;
  const caName = values.ca_certificate_name.trim() || null;
  const clientCertificateName = values.client_certificate_name.trim() || null;

  if (values.clear_ca_certificate && caCertificate) {
    return { error: "请先清空 CA 证书内容，或取消清除操作。" };
  }
  if (caCertificate && !caName) {
    return { error: "请输入 CA 证书名称。" };
  }
  if (
    caCertificate &&
    (!caCertificate.includes("-----BEGIN CERTIFICATE-----") ||
      !caCertificate.includes("-----END CERTIFICATE-----"))
  ) {
    return { error: "CA 证书必须是 PEM 格式。" };
  }

  if (values.clear_client_certificate && (clientCertificate || clientKey)) {
    return { error: "请先清空客户端证书和私钥内容，或取消清除操作。" };
  }
  if (Boolean(clientCertificate) !== Boolean(clientKey)) {
    return { error: "客户端证书和私钥必须同时填写。" };
  }
  if (clientCertificate && !clientCertificateName) {
    return { error: "请输入客户端证书名称。" };
  }
  if (
    clientCertificate &&
    (!clientCertificate.includes("-----BEGIN CERTIFICATE-----") ||
      !clientCertificate.includes("-----END CERTIFICATE-----"))
  ) {
    return { error: "客户端证书必须是 PEM 格式。" };
  }
  if (
    clientKey &&
    ![
      "-----BEGIN PRIVATE KEY-----",
      "-----BEGIN RSA PRIVATE KEY-----",
      "-----BEGIN EC PRIVATE KEY-----",
    ].some((marker) => clientKey.includes(marker))
  ) {
    return { error: "客户端私钥必须是 PEM 格式。" };
  }

  return {
    input: {
      profile: {
        ...(ssh || initial?.ssh ? { ssh } : {}),
        ...(sentinel || initial?.sentinel ? { sentinel } : {}),
        ...(cluster || initial?.cluster ? { cluster } : {}),
        id: profileId,
        name,
        host,
        port,
        username: values.username.trim() || null,
        database,
        has_password: Boolean(password) || Boolean(initial?.has_password),
        tls: values.tls,
        verify_server_cert: values.verify_server_cert,
        ca_certificate_name: values.clear_ca_certificate ? null : caName,
        client_certificate_name: values.clear_client_certificate
          ? null
          : clientCertificateName,
        has_ca_certificate:
          Boolean(caCertificate) ||
          Boolean(initial?.has_ca_certificate && !values.clear_ca_certificate),
        has_client_certificate:
          Boolean(clientCertificate && clientKey) ||
          Boolean(initial?.has_client_certificate && !values.clear_client_certificate),
      },
      password,
      clear_ssh_secrets: values.clear_ssh_secrets,
      ...(ssh ? {
        ssh_password: ssh.auth_method === "password" ? values.ssh_password || null : null,
        ssh_private_key: ssh.auth_method === "private_key" ? values.ssh_private_key.trim() || null : null,
        ssh_passphrase: ssh.auth_method === "private_key" ? values.ssh_passphrase || null : null,
        ssh_identity_file: ssh.auth_method === "private_key" ? values.ssh_identity_file.trim() || null : null,
        ssh_known_hosts_file: values.ssh_known_hosts_file.trim() || null,
      } : {}),
      ...(sentinel ? { sentinel_password: values.sentinel_password || null } : {}),
      ca_certificate: caCertificate,
      client_certificate: clientCertificate,
      client_key: clientKey,
      clear_ca_certificate: values.clear_ca_certificate,
      clear_client_certificate: values.clear_client_certificate,
    },
  };
}

export function ConnectionForm({
  initial,
  onSaved,
  onCancel,
  onConnect,
  onOpened,
  onOpenFailed,
  onTestingChange,
  onSavingChange,
}: ConnectionFormProps) {
  const [profileId] = useState(() => initial?.id ?? crypto.randomUUID());
  const [values, setValues] = useState<ConnectionFormValues>(() =>
    formValuesFromProfile(initial),
  );
  const [error, setError] = useState<string | null>(null);
  const [testStatus, setTestStatus] = useState<string | null>(null);
  const [testing, setTesting] = useState(false);
  const [saving, setSaving] = useState(false);

  const updateValue = (field: keyof ConnectionFormValues, value: string) => {
    setValues((current) => ({ ...current, [field]: value,
      ...(field === "topology" && value === "cluster" ? { database: "0", ssh_enabled: false } : {}),
      ...(field === "ssh_auth_method" ? { ssh_password: "", ssh_private_key: "", ssh_identity_file: "", ssh_passphrase: "" } : {}),
    }));
    setError(null);
    setTestStatus(null);
  };

  const updateBoolean = (field: keyof ConnectionFormValues, value: boolean) => {
    setValues((current) => ({ ...current, [field]: value }));
    setError(null);
    setTestStatus(null);
  };

  const handleTest = async () => {
    if (testing || saving) {
      return;
    }

    const validation = buildConnectionInput(values, profileId, initial);
    if (!validation.input) {
      setError(validation.error ?? "连接配置无效。");
      return;
    }

    setTesting(true);
    onTestingChange?.(true);
    setError(null);
    setTestStatus(null);
    try {
      const info = await testConnection(validation.input);
      setTestStatus(`连接成功，Redis ${info.server_version}${info.resolved_endpoint ? `，主节点 ${info.resolved_endpoint.host}:${info.resolved_endpoint.port}` : ""}`);
    } catch (caught) {
      setError(toUserFacingError(caught, "测试连接失败，请检查配置。"));
    } finally {
      setTesting(false);
      onTestingChange?.(false);
    }
  };

  const handleSave = async (connectAfterSave: boolean) => {
    if (testing || saving) {
      return;
    }

    const validation = buildConnectionInput(values, profileId, initial);
    if (!validation.input) {
      setError(validation.error ?? "连接配置无效。");
      return;
    }

    setSaving(true);
    onSavingChange?.(true);
    setError(null);
    setTestStatus(null);
    let saved: ConnectionProfile;
    try {
      saved = await saveConnection(validation.input);
    } catch (caught) {
      setError(toUserFacingError(caught, "保存连接失败，请稍后重试。"));
      setSaving(false);
      onSavingChange?.(false);
      return;
    }

    onSaved(saved);
    if (connectAfterSave) {
      try {
        if (onConnect) {
          await onConnect(saved);
        } else {
          await openConnection(saved.id);
          onOpened?.(saved);
        }
      } catch {
        if (onOpenFailed) {
          onOpenFailed(saved);
        } else {
          setError(savedButOpenFailedMessage);
        }
      }
    }

    setSaving(false);
    onSavingChange?.(false);
  };

  const busy = testing || saving;

  return (
    <section className="connection-form-panel" aria-labelledby="connection-form-title">
      <div className="panel-heading">
        <div>
          <p className="eyebrow">连接配置</p>
          <h2 id="connection-form-title">{initial ? "编辑连接" : "新增连接"}</h2>
        </div>
        <span className="panel-hint">本地保存连接配置</span>
      </div>

      <form
        className="connection-form"
        onSubmit={(event) => {
          event.preventDefault();
          void handleSave(false);
        }}
      >
        <div className="form-grid">
          <label className="field">
            <span>连接拓扑</span>
            <Select value={values.topology} onChange={(event) => updateValue("topology", event.target.value)} disabled={busy}>
              <option value="standalone">Standalone</option>
              <option value="sentinel">Sentinel</option>
              <option value="cluster">Cluster</option>
            </Select>
          </label>
          <label className="field">
            <span>连接名称</span>
            <input
              autoCapitalize="off"
              autoCorrect="off"
              autoComplete="off"
              value={values.name}
              onChange={(event) => updateValue("name", event.target.value)}
              placeholder="例如：本地 Redis"
              disabled={busy}
              required
            />
          </label>

          {values.topology === "standalone" && <label className="field">
            <span>主机</span>
            <input
              autoCapitalize="off"
              autoCorrect="off"
              autoComplete="off"
              value={values.host}
              onChange={(event) => updateValue("host", event.target.value)}
              placeholder="127.0.0.1"
              disabled={busy}
              required
            />
          </label>}

          {values.topology === "standalone" && <label className="field">
            <span>端口</span>
            <input
              autoCapitalize="off"
              autoCorrect="off"
              type="number"
              inputMode="numeric"
              min={1}
              max={65535}
              value={values.port}
              onChange={(event) => updateValue("port", event.target.value)}
              disabled={busy}
              required
            />
          </label>}

          <label className="field">
            <span>用户名</span>
            <input
              autoCapitalize="off"
              autoCorrect="off"
              autoComplete="username"
              value={values.username}
              onChange={(event) => updateValue("username", event.target.value)}
              placeholder="可选"
              disabled={busy}
            />
          </label>

          <label className="field">
            <span>数据库</span>
            <input
              autoCapitalize="off"
              autoCorrect="off"
              type="number"
              inputMode="numeric"
              min={0}
              max={15}
              value={values.topology === "cluster" ? "0" : values.database}
              onChange={(event) => updateValue("database", event.target.value)}
              disabled={busy || values.topology === "cluster"}
              required
            />
          </label>

          <label className="field">
            <span>密码</span>
            <input
              autoCapitalize="off"
              autoCorrect="off"
              type="password"
              autoComplete={initial ? "new-password" : "current-password"}
              value={values.password}
              onChange={(event) => updateValue("password", event.target.value)}
              placeholder={initial?.has_password ? "留空以保留现有密码" : "可选"}
              disabled={busy}
            />
          </label>
        </div>

        <section className="connection-tls-panel" aria-label="SSH 配置">
          <label className="checkbox-field"><input autoCapitalize="off" autoCorrect="off" type="checkbox" checked={values.ssh_enabled} onChange={(event) => updateBoolean("ssh_enabled", event.target.checked)} disabled={busy || values.topology === "cluster"} /><span>启用 SSH 隧道</span></label>
          {values.topology === "cluster" && <p className="field-hint">Cluster 暂不支持 SSH 隧道；请使用可直达各节点的网络。</p>}
          {values.ssh_enabled && <>
            <p>支持 Standalone / Sentinel 与 TLS 组合。请先通过可信渠道核验主机密钥并写入 known_hosts；应用不会自动信任未知主机。切换认证方式会清除其他方式的凭据。</p>
            <div className="form-grid">
              <label className="field"><span>SSH 主机</span><input autoCapitalize="off" autoCorrect="off" value={values.ssh_host} onChange={(event) => updateValue("ssh_host", event.target.value)} disabled={busy} /></label>
              <label className="field"><span>SSH 端口</span><input autoCapitalize="off" autoCorrect="off" type="number" min={1} max={65535} value={values.ssh_port} onChange={(event) => updateValue("ssh_port", event.target.value)} disabled={busy} /></label>
              <label className="field"><span>SSH 用户名</span><input autoCapitalize="off" autoCorrect="off" autoComplete="username" value={values.ssh_username} onChange={(event) => updateValue("ssh_username", event.target.value)} disabled={busy} /></label>
              <label className="field"><span>SSH 认证方式</span><Select value={values.ssh_auth_method} onChange={(event) => updateValue("ssh_auth_method", event.target.value)} disabled={busy}><option value="agent">Agent</option><option value="password">Password</option><option value="private_key">Private Key</option></Select></label>
              {values.ssh_auth_method === "password" && <label className="field"><span>SSH 密码</span><input autoCapitalize="off" autoCorrect="off" type="password" autoComplete="new-password" value={values.ssh_password} onChange={(event) => updateValue("ssh_password", event.target.value)} disabled={busy} placeholder="已保存时留空保留" /></label>}
              {values.ssh_auth_method === "private_key" && <>
                <label className="field"><span>SSH 私钥文件路径</span><input autoCapitalize="off" autoCorrect="off" value={values.ssh_identity_file} onChange={(event) => updateValue("ssh_identity_file", event.target.value)} placeholder="绝对路径，与私钥内容二选一" disabled={busy} /></label>
                <label className="field"><span>SSH 私钥内容</span><textarea autoCapitalize="off" autoCorrect="off" value={values.ssh_private_key} onChange={(event) => updateValue("ssh_private_key", event.target.value)} disabled={busy} rows={3} autoComplete="off" /></label>
                <label className="field"><span>SSH 私钥口令</span><input autoCapitalize="off" autoCorrect="off" type="password" autoComplete="new-password" value={values.ssh_passphrase} onChange={(event) => updateValue("ssh_passphrase", event.target.value)} disabled={busy} /></label>
              </>}
              <label className="field"><span>SSH 已知主机文件路径</span><input autoCapitalize="off" autoCorrect="off" value={values.ssh_known_hosts_file} onChange={(event) => updateValue("ssh_known_hosts_file", event.target.value)} placeholder="可选，默认 ~/.ssh/known_hosts" disabled={busy} /></label>
            </div>
            {initial?.ssh && <>
              <p className="field-hint">{values.clear_ssh_secrets ? "已选择清除旧材料，可填写替换材料。known_hosts 路径和私钥口令也会清除，请按需重新填写。" : "已保存材料仅显示状态，留空保留当前认证方式的材料。切换私钥内容与文件路径来源前，请先勾选清除。"}</p>
              {!values.clear_ssh_secrets && <p className="field-hint">{initial.ssh.auth_method === values.ssh_auth_method && [initial.ssh.has_password && "密码已保存", initial.ssh.has_private_key && "私钥已保存", initial.ssh.has_identity_file && "私钥路径已保存", initial.ssh.has_passphrase && "私钥口令已保存"].filter(Boolean).join(" · ")}{initial.ssh.has_known_hosts_file ? " · 已知主机路径已保存" : ""}</p>}
              <label className="checkbox-field"><input autoCapitalize="off" autoCorrect="off" type="checkbox" checked={values.clear_ssh_secrets} onChange={(event) => updateBoolean("clear_ssh_secrets", event.target.checked)} disabled={busy} /><span>清除已保存的 SSH 凭据和路径</span></label>
            </>}
          </>}
        </section>
        {values.topology === "cluster" && <section className="connection-tls-panel" aria-label="Cluster 配置">
          <h3>Cluster</h3><p>数据库固定为 DB 0。种子用于发现完整拓扑，每行一个节点。</p>
          <label className="field"><span>Cluster 种子节点</span><textarea autoCapitalize="off" autoCorrect="off" value={values.cluster_nodes} onChange={(event) => updateValue("cluster_nodes", event.target.value)} disabled={busy} rows={4} /></label>
          <label className="checkbox-field"><input autoCapitalize="off" autoCorrect="off" type="checkbox" checked={values.cluster_read_from_replicas} onChange={(event) => updateBoolean("cluster_read_from_replicas", event.target.checked)} disabled={busy} /><span>允许从副本读取</span></label>
        </section>}
        {values.topology === "sentinel" && <section className="connection-tls-panel" aria-label="Sentinel 配置">
          <h3>Sentinel</h3>
          <p>重新连接时发现当前主节点；主从切换后请重新连接，正在运行的会话不会自动迁移。</p>
          <div className="form-grid">
            <label className="field"><span>Sentinel 主节点名称</span><input autoCapitalize="off" autoCorrect="off" value={values.sentinel_master_name} onChange={(event) => updateValue("sentinel_master_name", event.target.value)} disabled={busy} placeholder="mymaster" /></label>
            <label className="field"><span>Sentinel 种子节点</span><textarea autoCapitalize="off" autoCorrect="off" value={values.sentinel_nodes} onChange={(event) => updateValue("sentinel_nodes", event.target.value)} disabled={busy} rows={3} /></label>
            <label className="field"><span>Sentinel 用户名</span><input autoCapitalize="off" autoCorrect="off" autoComplete="username" value={values.sentinel_username} onChange={(event) => updateValue("sentinel_username", event.target.value)} disabled={busy} placeholder="可选，与 Redis 用户名独立" /></label>
            <label className="field"><span>Sentinel 密码</span><input autoCapitalize="off" autoCorrect="off" type="password" autoComplete="new-password" value={values.sentinel_password} onChange={(event) => updateValue("sentinel_password", event.target.value)} disabled={busy} placeholder={initial?.sentinel?.has_password ? "留空保留已保存密码" : "可选"} /></label>
          </div>
          {initial?.sentinel?.has_password && <label className="checkbox-field"><input autoCapitalize="off" autoCorrect="off" type="checkbox" checked={values.clear_sentinel_password} onChange={(event) => updateBoolean("clear_sentinel_password", event.target.checked)} disabled={busy} /><span>清除已保存的 Sentinel 密码</span></label>}
          <label className="checkbox-field"><input autoCapitalize="off" autoCorrect="off" type="checkbox" checked={values.sentinel_tls} onChange={(event) => updateBoolean("sentinel_tls", event.target.checked)} disabled={busy} /><span>Sentinel 启用 TLS</span></label>
          <p className="field-hint">上方用户名和密码用于 Redis 主节点。Sentinel TLS 与主节点 TLS 独立启用，共用下方证书和校验设置。</p>
        </section>}
        <section className="connection-tls-panel" aria-labelledby="tls-panel-title">
          <div className="connection-tls-heading">
            <div>
              <p className="eyebrow">传输安全</p>
              <h3 id="tls-panel-title">TLS / 证书</h3>
            </div>
            <span className="panel-hint">host 同时作为 TLS SNI</span>
          </div>

          <label className="checkbox-field">
            <input
              autoCapitalize="off"
              autoCorrect="off"
              type="checkbox"
              aria-label="启用 TLS"
              checked={values.tls}
              onChange={(event) => updateBoolean("tls", event.target.checked)}
              disabled={busy}
            />
            <span>启用 TLS（rediss）</span>
          </label>

          {values.tls || (values.topology === "sentinel" && values.sentinel_tls) || initial?.has_ca_certificate || initial?.has_client_certificate ? (
            <div className="connection-tls-fields">
              <label className="checkbox-field">
                <input
                  autoCapitalize="off"
                  autoCorrect="off"
                  type="checkbox"
                  aria-label="验证服务端证书"
                  checked={values.verify_server_cert}
                  onChange={(event) =>
                    updateBoolean("verify_server_cert", event.target.checked)
                  }
                  disabled={busy}
                />
                <span>验证服务端证书</span>
              </label>
              {!values.verify_server_cert ? (
                <p className="form-help form-help-warning">
                  已关闭证书校验，仅建议用于明确受控的开发环境。
                </p>
              ) : null}

              <div className="form-grid">
                <label className="field">
                  <span>CA 名称</span>
                  <input
                    autoCapitalize="off"
                    autoCorrect="off"
                    autoComplete="off"
                    aria-label="CA 名称"
                    value={values.ca_certificate_name}
                    onChange={(event) =>
                      updateValue("ca_certificate_name", event.target.value)
                    }
                    placeholder="例如：Redis Root CA"
                    disabled={busy}
                  />
                </label>

                <label className="field field-wide">
                  <span>CA 证书</span>
                  <textarea
                    autoCapitalize="off"
                    autoCorrect="off"
                    aria-label="CA 证书"
                    value={values.ca_certificate}
                    onChange={(event) =>
                      updateValue("ca_certificate", event.target.value)
                    }
                    placeholder={
                      initial?.has_ca_certificate
                        ? "留空以保留现有 CA；正文不会回填"
                        : "粘贴 -----BEGIN CERTIFICATE----- PEM"
                    }
                    rows={5}
                    disabled={busy}
                  />
                </label>
              </div>

              {initial?.has_ca_certificate ? (
                <label className="checkbox-field">
                  <input
                    autoCapitalize="off"
                    autoCorrect="off"
                    type="checkbox"
                    aria-label="清除已有 CA 证书"
                    checked={values.clear_ca_certificate}
                    onChange={(event) =>
                      updateBoolean("clear_ca_certificate", event.target.checked)
                    }
                    disabled={busy}
                  />
                  <span>清除已有 CA 证书</span>
                </label>
              ) : null}

              <div className="form-grid">
                <label className="field">
                  <span>客户端证书名称</span>
                  <input
                    autoCapitalize="off"
                    autoCorrect="off"
                    autoComplete="off"
                    aria-label="客户端证书名称"
                    value={values.client_certificate_name}
                    onChange={(event) =>
                      updateValue("client_certificate_name", event.target.value)
                    }
                    placeholder="启用 mTLS 时填写"
                    disabled={busy}
                  />
                </label>

                <label className="field field-wide">
                  <span>客户端证书</span>
                  <textarea
                    autoCapitalize="off"
                    autoCorrect="off"
                    aria-label="客户端证书"
                    value={values.client_certificate}
                    onChange={(event) =>
                      updateValue("client_certificate", event.target.value)
                    }
                    placeholder={
                      initial?.has_client_certificate
                        ? "留空以保留现有证书；正文不会回填"
                        : "粘贴客户端证书 PEM"
                    }
                    rows={5}
                    disabled={busy}
                  />
                </label>

                <label className="field field-wide">
                  <span>客户端私钥</span>
                  <textarea
                    autoCapitalize="off"
                    autoCorrect="off"
                    aria-label="客户端私钥"
                    value={values.client_key}
                    onChange={(event) => updateValue("client_key", event.target.value)}
                    placeholder={
                      initial?.has_client_certificate
                        ? "留空以保留现有私钥；正文不会回填"
                        : "粘贴 PKCS#8 / RSA / EC 私钥 PEM"
                    }
                    rows={5}
                    disabled={busy}
                  />
                </label>
              </div>

              {initial?.has_client_certificate ? (
                <label className="checkbox-field">
                  <input
                    autoCapitalize="off"
                    autoCorrect="off"
                    type="checkbox"
                    aria-label="清除已有客户端证书"
                    checked={values.clear_client_certificate}
                    onChange={(event) =>
                      updateBoolean("clear_client_certificate", event.target.checked)
                    }
                    disabled={busy}
                  />
                  <span>清除已有客户端证书和私钥</span>
                </label>
              ) : null}
              <p className="form-help">
                证书正文仅提交到本机安全存储，不会出现在连接配置 JSON 或导出文件中。
              </p>
            </div>
          ) : null}
        </section>

        {error ? (
          <p className="feedback feedback-error" role="alert">
            {error}
          </p>
        ) : null}
        {testStatus ? (
          <p className="feedback feedback-success" role="status" aria-live="polite">
            {testStatus}
          </p>
        ) : null}

        <div className="form-actions">
          <button type="button" className="button button-quiet" onClick={onCancel} disabled={busy}>
            取消
          </button>
          <button
            type="submit"
            className="button button-secondary"
            disabled={busy}
          >
            {saving ? "保存中…" : "保存"}
          </button>
          <button
            type="button"
            className="button button-secondary"
            onClick={() => void handleTest()}
            disabled={busy}
          >
            {testing ? "测试中…" : "测试连接"}
          </button>
          <button
            type="button"
            className="button button-primary"
            onClick={() => void handleSave(true)}
            disabled={busy}
          >
            {saving ? "保存中…" : "保存并连接"}
          </button>
        </div>
      </form>
    </section>
  );
}

export default ConnectionForm;
