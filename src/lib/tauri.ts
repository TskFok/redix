import { invoke } from "@tauri-apps/api/core";

import type {
  CommandResult,
  ConnectionInfo,
  ConnectionProfile,
  DeleteKeyInput,
  ExecuteCommandInput,
  GetKeyInput,
  IpcError,
  KeyValue,
  SaveConnectionInput,
  ScanKeysInput,
  ScanPage,
  SetKeyInput,
  SetKeyTtlInput,
} from "./types";

const IPC_ERROR: IpcError = {
  code: "IPC_ERROR",
  message: "IPC 调用失败",
};

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

function normalizeError(error: unknown): IpcError {
  if (
    isRecord(error) &&
    typeof error.code === "string" &&
    typeof error.message === "string"
  ) {
    return {
      code: error.code,
      message: error.message,
    };
  }

  return { ...IPC_ERROR };
}

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return args === undefined ? await invoke<T>(command) : await invoke<T>(command, args);
  } catch (error) {
    throw normalizeError(error);
  }
}

export function listConnections(): Promise<ConnectionProfile[]> {
  return call<ConnectionProfile[]>("list_connections");
}

export function saveConnection(
  input: SaveConnectionInput,
): Promise<ConnectionProfile> {
  return call<ConnectionProfile>("save_connection", { input });
}

export function deleteConnection(connectionId: string): Promise<void> {
  return call<void>("delete_connection", { connection_id: connectionId });
}

export function testConnection(input: SaveConnectionInput): Promise<ConnectionInfo> {
  return call<ConnectionInfo>("test_connection", { input });
}

export function openConnection(connectionId: string): Promise<ConnectionInfo> {
  return call<ConnectionInfo>("open_connection", { connection_id: connectionId });
}

export function closeConnection(connectionId: string): Promise<void> {
  return call<void>("close_connection", { connection_id: connectionId });
}

export function scanKeys(input: ScanKeysInput): Promise<ScanPage> {
  return call<ScanPage>("scan_keys", { input });
}

export function getKey(input: GetKeyInput): Promise<KeyValue> {
  return call<KeyValue>("get_key", { input });
}

export function setKey(input: SetKeyInput): Promise<KeyValue> {
  return call<KeyValue>("set_key", { input });
}

export function deleteKey(input: DeleteKeyInput): Promise<void> {
  return call<void>("delete_key", { input });
}

export function setKeyTtl(input: SetKeyTtlInput): Promise<number> {
  return call<number>("set_key_ttl", { input });
}

export function executeCommand(input: ExecuteCommandInput): Promise<CommandResult> {
  return call<CommandResult>("execute_command", { input });
}
