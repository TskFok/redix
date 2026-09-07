import { useEffect, useRef, useState } from "react";
import { BULK_TASKS_CHANGED, startBulkDelete } from "./bulkTaskApi";
export default function StartBulkDeleteButton({ connectionId, keys, disabled }: { connectionId: string; keys: string[]; disabled: boolean }) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const generation = useRef(0);
  useEffect(() => { generation.current += 1; setBusy(false); setError(""); return () => { generation.current += 1; }; }, [connectionId]);
  const start = async () => {
    if (busy || disabled || !keys.length || !window.confirm(`在后台删除选中的 ${keys.length} 个键？操作无法撤销；可在后台任务中取消尚未开始的删除。`)) return;
    const token = generation.current;
    setBusy(true); setError("");
    try {
      await startBulkDelete(connectionId, [...keys]);
      window.dispatchEvent(new Event(BULK_TASKS_CHANGED));
    } catch { if (generation.current === token) setError("后台删除启动失败。最多选择 10000 个键，同时运行不超过 4 个任务。"); }
    finally { if (generation.current === token) setBusy(false); }
  };
  return <><button type="button" className="button button-danger" disabled={disabled || busy || !keys.length || keys.length > 10_000} onClick={() => void start()}>{busy ? "启动中…" : "后台批量删除"}</button>{error && <p role="alert">{error}</p>}</>;
}
