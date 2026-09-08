import { useEffect, useRef, useState } from "react";
import { useConfirmDialog } from "../../components/useConfirmDialog";

import { deleteKeys } from "../../lib/tauri";
import { browserErrorMessage } from "./browserState";

interface BulkKeyActionsProps {
  connectionId: string;
  selectedKeys: string[];
  busy: boolean;
  onDeleted: (count: number) => void;
  onError: (message: string) => void;
  onBusyChange?: (busy: boolean) => void;
}

export function BulkKeyActions({
  connectionId,
  selectedKeys,
  busy,
  onDeleted,
  onError,
  onBusyChange,
}: BulkKeyActionsProps) {
  const [submitting, setSubmitting] = useState(false);
  useEffect(() => { onBusyChange?.(submitting); }, [submitting, onBusyChange]);
  const { confirm, confirmationDialog } = useConfirmDialog(JSON.stringify([connectionId, selectedKeys, busy]));
  const inFlight = useRef(false);
  const generation = useRef(0);
  const mountedRef = useRef(false);
  const connectionRef = useRef(connectionId);
  connectionRef.current = connectionId;

  useEffect(() => {
    generation.current += 1;
    mountedRef.current = true;
    inFlight.current = false;
    setSubmitting(false);
    return () => {
      mountedRef.current = false;
      generation.current += 1;
    };
  }, [connectionId]);

  const handleDelete = async () => {
    if (busy || inFlight.current || selectedKeys.length === 0) {
      return;
    }
    const token = generation.current;
    const requestConnectionId = connectionId;
    const requestKeys = [...selectedKeys];
    if (!await confirm(`确定删除选中的 ${requestKeys.length} 个键吗？此操作无法撤销。`)) {
      return;
    }
    const isCurrent = () => mountedRef.current && generation.current === token && connectionRef.current === requestConnectionId;
    if (!isCurrent() || inFlight.current) return;
    inFlight.current = true;
    setSubmitting(true);
    try {
      const count = await deleteKeys({ connection_id: requestConnectionId, keys: requestKeys });
      if (isCurrent()) {
        onDeleted(count);
      }
    } catch (caught) {
      if (isCurrent()) {
        onError(browserErrorMessage(caught, "批量删除失败，请稍后重试。"));
      }
    } finally {
      if (isCurrent()) {
        inFlight.current = false;
        setSubmitting(false);
      }
    }
  };

  const isBusy = busy || submitting;
  const label = selectedKeys.length > 0 ? `批量删除（${selectedKeys.length}）` : "批量删除";

  return (
    <>
      <button type="button" className="button button-danger" onClick={() => void handleDelete()} disabled={isBusy || selectedKeys.length === 0}>
        {submitting ? "删除中…" : label}
      </button>
      {confirmationDialog}
    </>
  );
}

export default BulkKeyActions;
