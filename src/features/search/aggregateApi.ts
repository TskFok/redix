import { invoke } from "@tauri-apps/api/core";

export type AggregateFunction = "count" | "sum" | "min" | "max" | "avg";
export interface AggregateReducer { function: AggregateFunction; field: string | null; alias: string }
export interface AggregateSort { field: string; direction: "asc" | "desc" }
export interface SearchAggregateInput {
  connection_id: string;
  index: string;
  query: string;
  load_fields: string[];
  group_by: string[];
  reducers: AggregateReducer[];
  sort_by: AggregateSort[];
  offset: number;
  limit: number;
}
export interface SearchAggregateResult {
  offset: number;
  next_offset: number | null;
  columns: string[];
  rows: { fields: { name: string; value: unknown }[] }[];
}
export const aggregateSearch = (input: SearchAggregateInput) => invoke<SearchAggregateResult>("aggregate_search", { input });
