import { useRef, useState, type ChangeEvent } from "react";
import { useTransientFeedback } from "../../components/useTransientFeedback";
import { exportQueryPackage, importQueryPackage } from "../../lib/localProductsApi";
import type { QueryLibraryItem } from "../../lib/types";

const MAX_PACKAGE_BYTES = 10 * 1024 * 1024;

export default function QueryPackage({ onImported, disabled = false }: {onImported(items: QueryLibraryItem[]): void; disabled?: boolean}) {
  const [busy,setBusy] = useState(false);
  const busyRef = useRef(false);
  const [error,setError] = useState<string | null>(null);
  const [message,setMessage] = useTransientFeedback();
  const run = async (action: () => Promise<void>, fallback: string) => {
    if (busyRef.current || disabled) return;
    busyRef.current = true; setBusy(true); setError(null); setMessage(null);
    try { await action(); } catch { setError(fallback); }
    finally { busyRef.current = false; setBusy(false); }
  };
  const importFile = (event: ChangeEvent<HTMLInputElement>) => {
    const file = event.target.files?.[0]; event.target.value = "";
    if (!file || busyRef.current || disabled) return;
    if (file.size > MAX_PACKAGE_BYTES) {setError("查询包不能超过 10 MiB。");setMessage(null);return;}
    void run(async () => { const items = await importQueryPackage(await file.text()); onImported(items); setMessage(`已导入 ${items.length} 条查询。`); }, "查询包导入失败，请检查格式、版本、容量或敏感命令；已有查询保持不变。");
  };
  const exportFile = () => void run(async () => {
    const result = await exportQueryPackage();
    const payload = JSON.stringify(result,null,2);
    const blob = new Blob([payload],{type:"application/json"});
    const url = typeof URL.createObjectURL === "function" ? URL.createObjectURL(blob) : `data:application/json;charset=utf-8,${encodeURIComponent(payload)}`;
    const anchor = document.createElement("a"); anchor.href=url;anchor.download="redix-query-library.json";
    try { anchor.click(); } finally { if(url.startsWith("blob:") && typeof URL.revokeObjectURL === "function") URL.revokeObjectURL(url); }
    setMessage(`已导出 ${result.items.length} 条查询。`);
  }, "查询包导出失败，请检查本地查询和存储后重试。");
  return <section aria-label="查询包导入导出" aria-busy={busy}>
    <div className="card-actions">
      <label className="button button-secondary transfer-file-button">导入查询包<input autoCapitalize="off" autoCorrect="off" type="file" aria-label="导入查询包文件" accept="application/json,.json" disabled={busy || disabled} onChange={importFile}/></label>
      <button type="button" className="button button-secondary" disabled={busy || disabled} onClick={exportFile}>导出查询包</button>
    </div>
    <p className="panel-hint">查询包使用 Redix JSON 格式，导入后追加到当前查询库；不会执行命令。最多 500 条查询、文件上限 10 MiB。</p>
    {error && <p role="alert" className="feedback feedback-error">{error}</p>}
    {message && <p role="status">{message}</p>}
  </section>;
}
