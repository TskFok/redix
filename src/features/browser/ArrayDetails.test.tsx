import { cleanup, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import ArrayDetails from "./ArrayDetails";

const { getArraySummaryMock, getArrayRangeMock } = vi.hoisted(() => ({
  getArraySummaryMock: vi.fn(),
  getArrayRangeMock: vi.fn(),
}));

vi.mock("../../lib/tauri", () => ({
  getArraySummary: getArraySummaryMock,
  getArrayRange: getArrayRangeMock,
}));

describe("ArrayDetails", () => {
  afterEach(() => {
    cleanup();
    vi.clearAllMocks();
  });

  it("展示摘要、范围浏览和稀疏空槽位", async () => {
    getArraySummaryMock.mockResolvedValue({
      key: "events",
      length: "3",
      count: "2",
      next_index: "4",
    });
    getArrayRangeMock.mockResolvedValue({
      start: "0",
      end: "2",
      has_more: false,
      cells: [
        { index: "0", value: "created" },
        { index: "1", value: null },
        { index: "2", value: "updated" },
      ],
    });

    render(<ArrayDetails connectionId="local" keyName="events" />);

    expect(await screen.findByText("Array")).toBeInTheDocument();
    expect(await screen.findByText("created")).toBeInTheDocument();
    expect(screen.getByText("空槽位")).toBeInTheDocument();
    await waitFor(() => {
      expect(getArrayRangeMock).toHaveBeenCalledWith({
        connection_id: "local",
        key: "events",
        start: "0",
        end: "499",
      });
    });
  });
});
