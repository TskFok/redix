import type {
  ArrayAggregateResult,
  ArrayCell,
  ArrayRange,
  ArrayScan,
  ArraySearchResult,
  ArraySummary,
} from "../../lib/types";

export type ArrayDetailTab = "view" | "search" | "aggregate";

export interface ArrayRequestToken {
  connectionId: string;
  key: string;
  requestId: number;
}

export interface ArrayDetailState {
  activeTab: ArrayDetailTab;
  summary: ArraySummary | null;
  rangeStart: string;
  rangeEnd: string;
  cells: ArrayCell[];
  rangeHasMore: boolean;
  nextStart: string | null;
  search: ArraySearchResult | null;
  aggregate: ArrayAggregateResult | null;
  selectedIndices: string[];
  loading: boolean;
  error: string | null;
}

export function createArrayDetailState(summary: ArraySummary | null = null): ArrayDetailState {
  return {
    activeTab: "view",
    summary,
    rangeStart: "0",
    rangeEnd: "499",
    cells: [],
    rangeHasMore: false,
    nextStart: null,
    search: null,
    aggregate: null,
    selectedIndices: [],
    loading: false,
    error: null,
  };
}

export function applyArrayRange(
  current: ArrayDetailState,
  range: ArrayRange,
): ArrayDetailState {
  return {
    ...current,
    rangeStart: range.start,
    rangeEnd: range.end,
    cells: range.cells,
    rangeHasMore: range.has_more,
    nextStart: null,
    selectedIndices: current.selectedIndices.filter((index) =>
      range.cells.some((cell) => cell.index === index && cell.value !== null),
    ),
    loading: false,
    error: null,
  };
}

export function applyArrayScan(
  current: ArrayDetailState,
  scan: ArrayScan,
): ArrayDetailState {
  const cells = scan.elements.map((element) => ({
    index: element.index,
    value: element.value,
  }));
  return {
    ...current,
    cells,
    rangeStart: cells[0]?.index ?? current.rangeStart,
    rangeEnd: cells[cells.length - 1]?.index ?? current.rangeEnd,
    rangeHasMore: scan.has_more,
    nextStart: scan.next_start,
    selectedIndices: current.selectedIndices.filter((index) =>
      cells.some((cell) => cell.index === index),
    ),
    loading: false,
    error: null,
  };
}

export function applyArraySearch(
  current: ArrayDetailState,
  search: ArraySearchResult,
): ArrayDetailState {
  return {
    ...current,
    search,
    activeTab: "search",
    loading: false,
    error: null,
  };
}

export function applyArrayAggregate(
  current: ArrayDetailState,
  aggregate: ArrayAggregateResult,
): ArrayDetailState {
  return {
    ...current,
    aggregate,
    activeTab: "aggregate",
    loading: false,
    error: null,
  };
}

export function isCurrentArrayRequest(
  active: ArrayRequestToken | null,
  candidate: ArrayRequestToken,
): boolean {
  return (
    active !== null &&
    active.connectionId === candidate.connectionId &&
    active.key === candidate.key &&
    active.requestId === candidate.requestId
  );
}
