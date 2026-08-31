import { cleanup, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import VectorSetDetails from "./VectorSetDetails";

const {
  getVectorSetSummaryMock,
  listVectorSetElementsMock,
} = vi.hoisted(() => ({
  getVectorSetSummaryMock: vi.fn(),
  listVectorSetElementsMock: vi.fn(),
}));

vi.mock("../../lib/tauri", () => ({
  getVectorSetSummary: getVectorSetSummaryMock,
  listVectorSetElements: listVectorSetElementsMock,
}));

describe("VectorSetDetails", () => {
  afterEach(() => {
    cleanup();
    vi.clearAllMocks();
  });

  it("展示向量摘要、元素属性和分页状态", async () => {
    getVectorSetSummaryMock.mockResolvedValue({
      key: "embeddings",
      total: "2",
      dimension: 3,
      quantization: "f32",
    });
    listVectorSetElementsMock.mockResolvedValue({
      cursor: "(one",
      has_more: true,
      elements: [
        { name: "one", score: null, vector_base64: null, attributes: { label: "a" } },
      ],
    });

    render(<VectorSetDetails connectionId="local" keyName="embeddings" />);

    expect(await screen.findByText("Vector Set")).toBeInTheDocument();
    expect(await screen.findByText("one")).toBeInTheDocument();
    expect(screen.getByText('{"label":"a"}')).toBeInTheDocument();
    await waitFor(() => {
      expect(listVectorSetElementsMock).toHaveBeenCalledWith({
        connection_id: "local",
        key: "embeddings",
        start: null,
        end: null,
        limit: 50,
      });
    });
  });
});
