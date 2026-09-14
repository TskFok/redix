import { invokeBinary as invoke } from "../../lib/binaryIpc";
import type { RedisBytes } from "../../lib/redisBytes";

export type CollectionKind = "hash" | "list" | "set" | "zset";
export type CollectionOrder = "scan" | "score_asc" | "score_desc";

export interface CollectionPageInput {
  connection_id: string;
  key: string;
  kind: CollectionKind;
  cursor: string;
  count: number;
  pattern: RedisBytes;
  order?: CollectionOrder;
}

export interface CollectionEntry { id: RedisBytes; value: RedisBytes; score: number | null; ttl_ms: number | null }
export interface CollectionPage {
  entries: CollectionEntry[];
  next_cursor: string;
  has_more: boolean;
  total: string;
  ttl_ms: number;
  hash_field_ttl_supported: boolean | null;
}

export type CollectionMutation =
  | { operation: "hash_set"; field: RedisBytes; value: RedisBytes }
  | { operation: "hash_delete"; field: RedisBytes }
  | { operation: "hash_expire"; field: RedisBytes; ttl_ms: string }
  | { operation: "hash_persist"; field: RedisBytes }
  | { operation: "set_add" | "set_remove"; member: RedisBytes }
  | { operation: "zset_add"; member: RedisBytes; score: number }
  | { operation: "zset_remove"; member: RedisBytes }
  | { operation: "list_set"; index: string; value: RedisBytes }
  | { operation: "list_append"; value: RedisBytes; prepend: boolean }
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
