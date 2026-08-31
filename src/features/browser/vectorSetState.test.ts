import { describe, expect, it } from "vitest";

import type { VectorSetPage, VectorSimilarityResult, VectorSetSummary } from "../../lib/types";
import {
  applyVectorSetPage,
  applyVectorSimilarityResult,
  createVectorSetDetailState,
  isCurrentVectorSetRequest,
} from "./vectorSetState";

const summary: VectorSetSummary = {
  key: "embeddings",
  total: "2",
  dimension: 3,
  quantization: "f32",
};

describe("Vector Set 详情状态 helper", () => {
  it("创建空分页状态并应用元素页", () => {
    const initial = createVectorSetDetailState(summary);
    const page: VectorSetPage = {
      elements: [
        { name: "one", score: null, vector_base64: null, attributes: { label: "a" } },
      ],
      cursor: "(one",
      has_more: true,
    };

    const next = applyVectorSetPage(initial, page, true);

    expect(next.page).toEqual(page);
    expect(next.selectedElements).toEqual([]);
    expect(next.loading).toBe(false);
  });

  it("切换相似度结果时清空旧元素选择", () => {
    const initial = {
      ...createVectorSetDetailState(summary),
      selectedElements: ["old"],
    };
    const result: VectorSimilarityResult = {
      matches: [{ name: "new", score: 0.9, attributes: null }],
      has_more: false,
    };

    const next = applyVectorSimilarityResult(initial, result);

    expect(next.similarity).toEqual(result);
    expect(next.selectedElements).toEqual([]);
  });

  it("拒绝过期的 Vector Set 请求", () => {
    const active = { connectionId: "local", key: "embeddings", requestId: 4 };

    expect(isCurrentVectorSetRequest(active, active)).toBe(true);
    expect(
      isCurrentVectorSetRequest(active, {
        connectionId: "local",
        key: "other",
        requestId: 4,
      }),
    ).toBe(false);
  });
});
