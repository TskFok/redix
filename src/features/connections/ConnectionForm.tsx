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
      },
      password,
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
