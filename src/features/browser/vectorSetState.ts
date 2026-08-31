import type {
  VectorSetPage,
  VectorSetSummary,
  VectorSimilarityResult,
} from "../../lib/types";

export type VectorSetDetailTab = "elements" | "similarity";

export interface VectorSetRequestToken {
  connectionId: string;
  key: string;
  requestId: number;
}

export interface VectorSetDetailState {
  activeTab: VectorSetDetailTab;
  summary: VectorSetSummary | null;
  page: VectorSetPage | null;
  similarity: VectorSimilarityResult | null;
  selectedElements: string[];
  selectedElement: string | null;
  loading: boolean;
  error: string | null;
}

export function createVectorSetDetailState(
  summary: VectorSetSummary | null = null,
): VectorSetDetailState {
  return {
    activeTab: "elements",
    summary,
    page: null,
    similarity: null,
    selectedElements: [],
    selectedElement: null,
    loading: false,
    error: null,
  };
}

export function applyVectorSetPage(
  current: VectorSetDetailState,
  page: VectorSetPage,
  replace: boolean,
): VectorSetDetailState {
  const elements = replace
    ? page.elements
    : [
        ...(current.page?.elements ?? []),
        ...page.elements.filter(
          (element) => !(current.page?.elements ?? []).some((item) => item.name === element.name),
        ),
      ];
  return {
    ...current,
    activeTab: "elements",
    page: { ...page, elements },
    similarity: replace ? null : current.similarity,
    selectedElements: replace
      ? []
      : current.selectedElements.filter((name) => elements.some((item) => item.name === name)),
    loading: false,
    error: null,
  };
}

export function applyVectorSimilarityResult(
  current: VectorSetDetailState,
  similarity: VectorSimilarityResult,
): VectorSetDetailState {
  return {
    ...current,
    activeTab: "similarity",
    similarity,
    selectedElements: [],
    selectedElement: null,
    loading: false,
    error: null,
  };
}

export function isCurrentVectorSetRequest(
  active: VectorSetRequestToken | null,
  candidate: VectorSetRequestToken,
): boolean {
  return (
    active !== null &&
    active.connectionId === candidate.connectionId &&
    active.key === candidate.key &&
    active.requestId === candidate.requestId
  );
}
