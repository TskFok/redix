import { useState } from "react";

import {
  openConnection,
  saveConnection,
  testConnection,
} from "../../lib/tauri";
import type { ConnectionProfile, SaveConnectionInput } from "../../lib/types";
import {
  formValuesFromProfile,
  savedButOpenFailedMessage,
  toUserFacingError,
  type ConnectionFormValues,
} from "./connectionState";

interface ConnectionFormProps {
  initial?: ConnectionProfile;
  onSaved: (profile: ConnectionProfile) => void;
  onCancel: () => void;
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
  initial?: ConnectionProfile,
): FormValidation {
  const name = values.name.trim();
  const host = values.host.trim();
  const port = Number(values.port);
  const database = Number(values.database);

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
        id: initial?.id ?? crypto.randomUUID(),
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
  onOpened,
  onOpenFailed,
  onTestingChange,
  onSavingChange,
}: ConnectionFormProps) {
  const [values, setValues] = useState<ConnectionFormValues>(() =>
    formValuesFromProfile(initial),
  );
  const [error, setError] = useState<string | null>(null);
  const [testStatus, setTestStatus] = useState<string | null>(null);
  const [testing, setTesting] = useState(false);
  const [saving, setSaving] = useState(false);

  const updateValue = (field: keyof ConnectionFormValues, value: string) => {
    setValues((current) => ({ ...current, [field]: value }));
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

    const validation = buildConnectionInput(values, initial);
    if (!validation.input) {
      setError(validation.error ?? "连接配置无效。");
      return;
    }
    if (initial?.has_password && !values.password) {
      setError("请输入密码后再测试连接。");
      return;
    }

    setTesting(true);
    onTestingChange?.(true);
    setError(null);
    setTestStatus(null);
    try {
      const info = await testConnection(validation.input);
      setTestStatus(`连接成功，Redis ${info.server_version}`);
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

    const validation = buildConnectionInput(values, initial);
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
        await openConnection(saved.id);
        onOpened?.(saved);
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
            <span>连接名称</span>
            <input
              autoComplete="off"
              value={values.name}
              onChange={(event) => updateValue("name", event.target.value)}
              placeholder="例如：本地 Redis"
              disabled={busy}
              required
            />
          </label>

          <label className="field">
            <span>主机</span>
            <input
              autoComplete="off"
              value={values.host}
              onChange={(event) => updateValue("host", event.target.value)}
              placeholder="127.0.0.1"
              disabled={busy}
              required
            />
          </label>

          <label className="field">
            <span>端口</span>
            <input
              type="number"
              inputMode="numeric"
              min={1}
              max={65535}
              value={values.port}
              onChange={(event) => updateValue("port", event.target.value)}
              disabled={busy}
              required
            />
          </label>

          <label className="field">
            <span>用户名</span>
            <input
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
              type="number"
              inputMode="numeric"
              min={0}
              max={15}
              value={values.database}
              onChange={(event) => updateValue("database", event.target.value)}
              disabled={busy}
              required
            />
          </label>

          <label className="field">
            <span>密码</span>
            <input
              type="password"
              autoComplete={initial ? "new-password" : "current-password"}
              value={values.password}
              onChange={(event) => updateValue("password", event.target.value)}
              placeholder={initial?.has_password ? "留空以保留现有密码" : "可选"}
              disabled={busy}
            />
          </label>
        </div>

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
              type="checkbox"
              aria-label="启用 TLS"
              checked={values.tls}
              onChange={(event) => updateBoolean("tls", event.target.checked)}
              disabled={busy}
            />
            <span>启用 TLS（rediss）</span>
          </label>

          {values.tls || initial?.has_ca_certificate || initial?.has_client_certificate ? (
            <div className="connection-tls-fields">
              <label className="checkbox-field">
                <input
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
