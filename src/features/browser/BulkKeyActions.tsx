import { useEffect, useRef, useState } from "react";

import { deleteKeys } from "../../lib/tauri";
import { browserErrorMessage } from "./browserState";

interface BulkKeyActionsProps {
  connectionId: string;
  selectedKeys: string[];
  busy: boolean;
  onDeleted: (count: number) => void;
  onError: (message: string) => void;
}

export function BulkKeyActions({
  connectionId,
  selectedKeys,
  busy,
  onDeleted,
  onError,
}: BulkKeyActionsProps) {
  const [submitting, setSubmitting] = useState(false);
  const mountedRef = useRef(false);
  const connectionRef = useRef(connectionId);
  connectionRef.current = connectionId;

  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
    };
  }, []);

  const handleDelete = async () => {
    if (selectedKeys.length === 0 || !window.confirm(`确定删除选中的 ${selectedKeys.length} 个键吗？`)) {
      return;
    }
    const requestConnectionId = connectionId;
    const requestKeys = [...selectedKeys];
    setSubmitting(true);
    try {
      const count = await deleteKeys({ connection_id: requestConnectionId, keys: requestKeys });
      if (mountedRef.current && connectionRef.current === requestConnectionId) {
        onDeleted(count);
      }
    } catch (caught) {
      if (mountedRef.current && connectionRef.current === requestConnectionId) {
        onError(browserErrorMessage(caught, "批量删除失败，请稍后重试。"));
      }
    } finally {
      if (mountedRef.current && connectionRef.current === requestConnectionId) {
        setSubmitting(false);
      }
    }
  };

  const isBusy = busy || submitting;
  const label = selectedKeys.length > 0 ? `批量删除（${selectedKeys.length}）` : "批量删除";

  return (
    <button type="button" className="button button-danger" onClick={() => void handleDelete()} disabled={isBusy || selectedKeys.length === 0}>
      {submitting ? "删除中…" : label}
    </button>
  );
}

export default BulkKeyActions;
