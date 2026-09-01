import { invoke } from "@tauri-apps/api/core";
export interface StreamEntryField { field: string; value: string }
export interface StreamEntryRecord { id: string; fields: StreamEntryField[] }
export interface StreamEntriesPage { entries: StreamEntryRecord[]; next_cursor: string | null; has_more: boolean }
export interface GetStreamEntriesInput { connection_id: string; key: string; start: string; end: string; cursor: string | null; count: number; reverse: boolean }
export interface AddStreamEntryInput { connection_id: string; key: string; id: string; fields: StreamEntryField[] }
export interface DeleteStreamEntriesInput { connection_id: string; key: string; ids: string[] }
export const getStreamEntries = (input: GetStreamEntriesInput) => invoke<StreamEntriesPage>("get_stream_entries", { input });
export const addStreamEntry = (input: AddStreamEntryInput) => invoke<string>("add_stream_entry", { input });
export const deleteStreamEntries = (input: DeleteStreamEntriesInput) => invoke<number>("delete_stream_entries", { input });
