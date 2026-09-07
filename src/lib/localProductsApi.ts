import { invoke } from "@tauri-apps/api/core";
import type { QueryLibraryItem } from "./types";

export interface ConnectionTag { key: string; value: string }
export type ConnectionTags = Record<string, ConnectionTag[]>;
export interface QueryPackageDocument {
  format: "redix-query-library";
  version: 1;
  items: { name: string; command: string; tags: string[] }[];
}

export const listConnectionTags = () => invoke<ConnectionTags>("list_connection_tags");
export const saveConnectionTags = (connectionId: string, tags: ConnectionTag[]) =>
  invoke<ConnectionTag[]>("save_connection_tags", { input: { connection_id: connectionId, tags } });
export const exportQueryPackage = () => invoke<QueryPackageDocument>("export_query_package");
export const importQueryPackage = (content: string) =>
  invoke<QueryLibraryItem[]>("import_query_package", { content });
