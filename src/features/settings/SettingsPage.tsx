import Select from "../../components/Select";
import { useEffect, useState } from "react";

import { saveAppSettings } from "../../lib/tauri";
import type { AppSettings } from "../../lib/types";
import {
  normalizeSettingsError,
  validateAppSettings,
} from "./settingsState";

interface SettingsPageProps {
  settings: AppSettings;
  onSaved(settings: AppSettings): void;
}

export function SettingsPage({ settings, onSaved }: SettingsPageProps) {
  const [draft, setDraft] = useState<AppSettings>(settings);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    setDraft(settings);
  }, [settings]);

  const handleSave = async () => {
    const validationError = validateAppSettings(draft);
    if (validationError) {
      setError(validationError);
      return;
    }

    setSaving(true);
    setError(null);
    try {
      const saved = await saveAppSettings(draft);
      onSaved(saved);
      setDraft(saved);
    } catch (caught) {
      setError(normalizeSettingsError(caught).message);
    } finally {
      setSaving(false);
    }
  };

  return (
    <section className="settings-page" aria-labelledby="settings-page-title">
      <div className="page-heading">
        <div>
          <p className="eyebrow">APPLICATION SETTINGS</p>
          <h2 id="settings-page-title">设置</h2>
          <p className="page-description">调整主题、结果展示和本地 Redis 工作区偏好。</p>
        </div>
      </div>

      {error ? (
        <p className="feedback feedback-error" role="alert">
          {error}
        </p>
      ) : null}

      <div className="settings-layout">
        <section className="settings-panel" aria-labelledby="appearance-settings-title">
          <div className="panel-heading">
            <div>
              <p className="eyebrow">APPEARANCE</p>
              <h2 id="appearance-settings-title">外观与结果</h2>
            </div>
          </div>
          <label className="field">
            <span>主题</span>
            <Select
              aria-label="主题"
              value={draft.theme}
              onChange={(event) =>
                setDraft((current) => ({
                  ...current,
                  theme: event.target.value as AppSettings["theme"],
                }))
              }
            >
              <option value="system">跟随系统</option>
              <option value="light">浅色</option>
              <option value="dark">深色</option>
            </Select>
          </label>
          <label className="field">
            <span>结果格式</span>
            <Select
              aria-label="结果格式"
              value={draft.result_format}
              onChange={(event) =>
                setDraft((current) => ({
                  ...current,
                  result_format: event.target.value as AppSettings["result_format"],
                }))
              }
            >
              <option value="raw">Raw</option>
              <option value="text">Text</option>
              <option value="json">JSON</option>
            </Select>
          </label>
        </section>

        <section className="settings-panel" aria-labelledby="workspace-settings-title">
          <div className="panel-heading">
            <div>
              <p className="eyebrow">WORKSPACE</p>
              <h2 id="workspace-settings-title">工作区偏好</h2>
            </div>
          </div>
          <label className="field">
            <span>每次扫描数量</span>
            <input
              autoCapitalize="off"
              autoCorrect="off"
              aria-label="每次扫描数量"
              type="number"
              min={10}
              max={1000}
              step={10}
              value={draft.scan_count}
              onChange={(event) =>
                setDraft((current) => ({
                  ...current,
                  scan_count: Number(event.target.value),
                }))
              }
            />
            <small>Browser 每次向 Redis 请求的 SCAN 数量，范围为 10 到 1000。</small>
          </label>
          <label className="field settings-checkbox">
            <span>
              <input
                autoCapitalize="off"
                autoCorrect="off"
                aria-label="批量命令遇错后继续"
                type="checkbox"
                checked={draft.continue_on_error}
                onChange={(event) =>
                  setDraft((current) => ({
                    ...current,
                    continue_on_error: event.target.checked,
                  }))
                }
              />
              批量命令遇错后继续
            </span>
          </label>
        </section>
      </div>

      <div className="form-actions">
        <button
          type="button"
          className="button button-primary"
          disabled={saving}
          onClick={() => void handleSave()}
        >
          保存设置
        </button>
      </div>
    </section>
  );
}

export default SettingsPage;
