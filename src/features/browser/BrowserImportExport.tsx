import { useRef, useState } from "react";
import Toast from "../../components/Toast";
import { useFeedbackState } from "../../components/useFeedbackState";
import { useTransientFeedback } from "../../components/useTransientFeedback";

import { exportKeys, importKeys } from "../../lib/tauri";
import type { ExportedKey, ImportKeysInput } from "../../lib/types";
import { browserErrorMessage } from "./browserState";

interface BrowserImportExportProps {
  connectionId: string;
  selectedKeys: string[];
  onImported: () => Promise<void>;
  disabled?: boolean;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

function parseImportEntries(raw: string): ImportKeysInput["entries"] {
  const parsed: unknown = JSON.parse(raw);
  if (!Array.isArray(parsed) || parsed.length === 0) {
    throw new Error("导入文件必须是非空键数组。");
  }

  const keys = new Set<string>();
  for (const item of parsed) {
    if (!isRecord(item) || typeof item.key !== "string" || item.key.trim() === "") {
      throw new Error("导入文件包含无效键名。");
    }
    if (keys.has(item.key)) {
      throw new Error("导入文件中存在重复键。");
    }
    keys.add(item.key);
    if (
      typeof item.ttl_ms !== "number" ||
      !Number.isInteger(item.ttl_ms) ||
      item.ttl_ms < -1 ||
      !isRecord(item.value)
    ) {
      throw new Error(`导入键“${item.key}”包含无效数据。`);
    }
  }

  return parsed as ExportedKey[];
}

export function BrowserImportExport({
  connectionId,
  selectedKeys,
  onImported,
  disabled = false,
}: BrowserImportExportProps) {
  const [busy, setBusy] = useState(false);
  const [error, setError, errorToken] = useFeedbackState<string | null>(null);
  const [status, setStatus, statusToken] = useTransientFeedback();
  const inputRef = useRef<HTMLInputElement>(null);

  const handleExport = async () => {
    if (selectedKeys.length === 0) {
      setError("请先选择要导出的键。");
      setStatus(null);
      return;
    }

    setBusy(true);
    setError(null);
    setStatus(null);
    try {
      const entries = await exportKeys({
        connection_id: connectionId,
        keys: [...selectedKeys],
      });
      const blob = new Blob([JSON.stringify(entries, null, 2)], {
        type: "application/json;charset=utf-8",
      });
      const url = URL.createObjectURL(blob);
      const link = document.createElement("a");
      link.href = url;
      link.download = "redix-keys.json";
      try {
        link.click();
      } finally {
        URL.revokeObjectURL(url);
      }
      setStatus(`已导出 ${entries.length} 个键。`);
    } catch (caught) {
      setError(browserErrorMessage(caught, "导出键失败，请稍后重试。"));
    } finally {
      setBusy(false);
    }
  };

  const handleImport = async (file: File) => {
    setBusy(true);
    setError(null);
    setStatus(null);
    try {
      const entries = parseImportEntries(await file.text());
      const imported = await importKeys({
        connection_id: connectionId,
        entries,
      });
      await onImported();
      setStatus(`已导入 ${imported} 个键，已跳过已存在的键。`);
    } catch (caught) {
      setError(browserErrorMessage(caught, "导入键失败，请检查 JSON 文件后重试。"));
    } finally {
      setBusy(false);
      if (inputRef.current) {
        inputRef.current.value = "";
      }
    }
  };

  const isBusy = disabled || busy;

  return (
    <div className="browser-file-actions" aria-label="Browser 文件操作">
      <button
        type="button"
        className="button button-secondary"
        onClick={() => void handleExport()}
        disabled={isBusy || selectedKeys.length === 0}
      >
        {busy ? "处理中…" : "导出选中键"}
      </button>
      <label className="button button-secondary browser-file-input-label">
        导入 JSON 文件
        <input
          autoCapitalize="off"
          autoCorrect="off"
          ref={inputRef}
          className="browser-file-input"
          type="file"
          accept="application/json"
          aria-label="导入 JSON 文件"
          onChange={(event) => {
            const file = event.target.files?.[0];
            if (file) {
              void handleImport(file);
            }
          }}
          disabled={isBusy}
        />
      </label>
      {error ? <Toast kind="error" message={error} onClose={() => setError(null)} resetKey={errorToken} /> : null}
      {status ? <Toast kind="success" message={status} onClose={() => setStatus(null)} resetKey={statusToken} /> : null}
    </div>
  );
}

export default BrowserImportExport;
