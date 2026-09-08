import { useLayoutEffect, useRef, useState } from "react";

import Select from "../../components/Select";
import Toast from "../../components/Toast";
import { selectDatabase } from "../../lib/tauri";
import type { ConnectionProfile } from "../../lib/types";

interface BrowserDatabaseSelectProps {
  connectionId: string;
  activeDatabase: number;
  isCluster?: boolean;
  disabled?: boolean;
  onProfileChanged: (profile: ConnectionProfile) => void;
  onSwitchingChange: (switching: boolean) => void;
}

const databases = Array.from({ length: 16 }, (_, database) => database);

export default function BrowserDatabaseSelect({
  connectionId,
  activeDatabase,
  isCluster = false,
  disabled = false,
  onProfileChanged,
  onSwitchingChange,
}: BrowserDatabaseSelectProps) {
  const [switching, setSwitching] = useState(false);
  const [error, setError] = useState(false);
  const mountedRef = useRef(false);
  const requestRef = useRef<symbol | null>(null);

  useLayoutEffect(() => {
    mountedRef.current = true;
    requestRef.current = null;
    setSwitching(false);
    setError(false);
    return () => {
      mountedRef.current = false;
      requestRef.current = null;
    };
  }, [connectionId]);

  const handleChange = async (database: number) => {
    if (disabled || isCluster || requestRef.current !== null || database === activeDatabase
      || !Number.isInteger(database) || database < 0 || database > 15) {
      return;
    }

    const request = Symbol();
    requestRef.current = request;
    setSwitching(true);
    setError(false);
    onSwitchingChange(true);
    try {
      const profile = await selectDatabase({ connection_id: connectionId, database });
      // The backend has changed even if this selector was left while awaiting it.
      onProfileChanged(profile);
    } catch {
      if (mountedRef.current && requestRef.current === request) setError(true);
    } finally {
      if (mountedRef.current && requestRef.current === request) {
        requestRef.current = null;
        setSwitching(false);
      }
      // Release the connection-level lock even after this selector unmounts.
      onSwitchingChange(false);
    }
  };

  return (
    <>
      <label className="browser-database-select">
        <span>Db</span>
        <Select
          aria-label="切换数据库"
          aria-busy={switching}
          title={isCluster ? "Cluster 仅支持 DB 0" : undefined}
          value={isCluster ? 0 : activeDatabase}
          disabled={disabled || switching || isCluster}
          onChange={(event) => void handleChange(Number(event.target.value))}
        >
          {(isCluster ? [0] : databases).map((database) => (
            <option key={database} value={database}>DB {database}</option>
          ))}
        </Select>
      </label>
      {error && (
        <Toast
          kind="error"
          message="数据库切换失败，已保留当前 Db。"
          onClose={() => setError(false)}
        />
      )}
    </>
  );
}
