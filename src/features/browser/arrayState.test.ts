import { describe, expect, it } from "vitest";

import type { ArrayRange, ArrayScan, ArraySummary } from "../../lib/types";
import {
  applyArrayRange,
  applyArrayScan,
  createArrayDetailState,
  isCurrentArrayRequest,
} from "./arrayState";

const summary: ArraySummary = {
  key: "events",
  length: "10",
  count: "8",
  next_index: "10",
};

describe("Array 详情状态 helper", () => {
  it("以摘要创建安全的首屏范围", () => {
    const state = createArrayDetailState(summary);

    expect(state.rangeStart).toBe("0");
    expect(state.rangeEnd).toBe("499");
    expect(state.summary).toEqual(summary);
  });

  it("应用范围和稀疏扫描结果时保留空槽位", () => {
    const initial = createArrayDetailState(summary);
    const range: ArrayRange = {
      start: "4",
      end: "6",
      cells: [
        { index: "4", value: "a" },
        { index: "5", value: null },
        { index: "6", value: "c" },
      ],
      has_more: false,
    };
    const scan: ArrayScan = {
      elements: [{ index: "8", value: "z" }],
      next_start: null,
      has_more: false,
    };

    const ranged = applyArrayRange(initial, range);
    const scanned = applyArrayScan(ranged, scan);

    expect(ranged.cells).toEqual(range.cells);
    expect(scanned.cells).toEqual([{ index: "8", value: "z" }]);
    expect(scanned.loading).toBe(false);
  });

  it("拒绝不同连接、键或序号的过期响应", () => {
    const active = { connectionId: "local", key: "events", requestId: 2 };

    expect(isCurrentArrayRequest(active, active)).toBe(true);
    expect(
      isCurrentArrayRequest(active, {
        connectionId: "remote",
        key: "events",
        requestId: 2,
      }),
    ).toBe(false);
    expect(
      isCurrentArrayRequest(active, {
        connectionId: "local",
        key: "events",
        requestId: 1,
      }),
    ).toBe(false);
  });
});
