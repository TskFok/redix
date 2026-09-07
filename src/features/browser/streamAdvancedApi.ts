import { invoke } from "@tauri-apps/api/core";
import type { StreamPendingEntry } from "../../lib/types";

export interface StreamGroupTarget { connection_id: string; key: string; group: string }
export interface StreamPendingPageInput extends StreamGroupTarget {
  consumer: string | null;
  start: string;
  end: string;
  cursor: string | null;
  count: number;
}
export interface StreamPendingPage { entries: StreamPendingEntry[]; next_cursor: string | null; has_more: boolean }
export interface StreamClaimAdvancedInput extends StreamGroupTarget {
  consumer: string;
  min_idle_ms: number;
  entries: string[];
  idle_ms: number | null;
  time_ms: number | null;
  retry_count: number | null;
  force: boolean;
}

export const getStreamPendingPage = (input: StreamPendingPageInput) =>
  invoke<StreamPendingPage>("get_stream_pending_page", { input });
export const claimStreamPendingAdvanced = (input: StreamClaimAdvancedInput) =>
  invoke<string[]>("claim_stream_pending_advanced", { input });
export const updateStreamGroupId = (input: StreamGroupTarget & {last_delivered_id: string}) =>
  invoke<void>("update_stream_group_id", { input });
