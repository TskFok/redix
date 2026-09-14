import { invoke } from "@tauri-apps/api/core";

export type CollectionKind = "hash" | "list" | "set" | "zset";
export type CollectionOrder = "scan" | "score_asc" | "score_desc";

export interface CollectionPageInput {
  connection_id: string;
  key: string;
  kind: CollectionKind;
  cursor: string;
  count: number;
  pattern: string;
  order?: CollectionOrder;
}

export interface CollectionEntry { id: string; value: string; score: number | null; ttl_ms: number | null }
export interface CollectionPage {
  entries: CollectionEntry[];
  next_cursor: string;
  has_more: boolean;
  total: string;
  ttl_ms: number;
  hash_field_ttl_supported: boolean | null;
}

export type CollectionMutation =
  | { operation: "hash_set"; field: string; value: string }
  | { operation: "hash_delete"; field: string }
  | { operation: "hash_expire"; field: string; ttl_ms: string }
  | { operation: "hash_persist"; field: string }
  | { operation: "set_add" | "set_remove"; member: string }
  | { operation: "zset_add"; member: string; score: number }
  | { operation: "zset_remove"; member: string }
  | { operation: "list_set"; index: string; value: string }
  | { operation: "list_append"; value: string; prepend: boolean }
  | { operation: "list_trim"; count: string; from_head: boolean };

export interface CollectionMutationInput {
  connection_id: string;
  key: string;
  mutation: CollectionMutation;
}

export function getCollectionPage(input: CollectionPageInput) {
  return invoke<CollectionPage>("get_collection_page", { input });
}

export function mutateCollection(input: CollectionMutationInput) {
  return invoke<void>("mutate_collection", { input });
}

export function getListEntry(input: { connection_id: string; key: string; index: string }) {
  return invoke<CollectionEntry | null>("get_list_entry", { input });
}
