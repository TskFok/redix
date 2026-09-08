import { useEffect, useRef, useState } from "react";
import { useConfirmDialog } from "../../components/useConfirmDialog";
import Toast from "../../components/Toast";
import { useFeedbackState } from "../../components/useFeedbackState";
import { BULK_TASKS_CHANGED, startBulkDelete } from "./bulkTaskApi";

export default function StartBulkDeleteButton({ connectionId, keys, disabled }: { connectionId: string; keys: string[]; disabled: boolean }) {
  const [busy, setBusy] = useState(false);
  const [error, setError, errorToken] = useFeedbackState("");
  const { confirm, confirmationDialog } = useConfirmDialog(JSON.stringify([connectionId, keys, disabled]));
  const inFlight = useRef(false);
  const generation = useRef(0);

  useEffect(() => {
    generation.current += 1;
    inFlight.current = false;
    setBusy(false);
    setError("");
    return () => { generation.current += 1; };
  }, [connectionId]);

  const start = async () => {
    if (busy || inFlight.current || disabled || !keys.length || keys.length > 10_000) return;
    const token = generation.current;
    const requestConnectionId = connectionId;
    const requestKeys = [...keys];
    if (!await confirm(`在后台删除选中的 ${requestKeys.length} 个键？操作无法撤销；可在后台任务中取消尚未开始的删除。`)) return;
    if (generation.current !== token || inFlight.current) return;
    inFlight.current = true;
    setBusy(true);
    setError("");
    try {
      await startBulkDelete(requestConnectionId, requestKeys);
      window.dispatchEvent(new Event(BULK_TASKS_CHANGED));
    } catch {
      if (generation.current === token) {
        setError("后台删除启动失败。最多选择 10000 个键，同时运行不超过 4 个任务。");
      }
    } finally {
      if (generation.current === token) {
        inFlight.current = false;
        setBusy(false);
      }
    }
  };

  return <>
    <button type="button" className="button button-danger" disabled={disabled || busy || !keys.length || keys.length > 10_000} onClick={() => void start()}>
      {busy ? "启动中…" : "后台批量删除"}
    </button>
    {confirmationDialog}
    {error && <Toast kind="error" message={error} resetKey={errorToken} onClose={() => setError("")} />}
  </>;
}
