import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { VectorSearchPanel } from "./VectorSearchPanel";
import { searchVectorIndex } from "./searchVectorApi";
import type { SearchIndexAttribute } from "../../lib/types";
vi.mock("./searchVectorApi", () => ({ searchVectorIndex: vi.fn() }));
const field: SearchIndexAttribute = { identifier: "$.embedding", query_name: "embedding", field_type: "VECTOR", sortable: false, no_index: false, vector: { data_type: "FLOAT32", dimension: 2, distance_metric: "L2" } };
const props = { connectionId: "local", index: "idx", attributes: [field], enabled: true };

afterEach(() => { cleanup(); vi.clearAllMocks(); });

describe("VectorSearchPanel", () => {
  it("显式提交用户向量与原生过滤，显示距离", async () => {
    vi.mocked(searchVectorIndex).mockResolvedValue({ matches: [{ key: "doc:1", distance: 0.25 }], returned: 1, count: 5, distance_metric: "L2" });
    render(<VectorSearchPanel {...props} />);
    fireEvent.change(screen.getByLabelText("查询向量（JSON 数组）"), { target: { value: "[1,0]" } });
    fireEvent.change(screen.getByLabelText("向量过滤条件"), { target: { value: "@tag:{book}" } });
    expect(searchVectorIndex).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "执行 KNN 查询" }));
    expect(await screen.findByText("doc:1")).toBeInTheDocument();
    expect(searchVectorIndex).toHaveBeenCalledWith({ connection_id: "local", index: "idx", field: "embedding", vector: [1, 0], count: 5, filter: "@tag:{book}" });
    expect(screen.getByText("0.25")).toBeInTheDocument();
  });
  it("拒绝维度不符、FLOAT32 溢出和过大 Top K", () => {
    render(<VectorSearchPanel {...props} />);
    for (const value of ["[1]", "[1e100,0]", "[null,0]"]) {
      fireEvent.change(screen.getByLabelText("查询向量（JSON 数组）"), { target: { value } });
      expect(screen.getByRole("button", { name: "执行 KNN 查询" })).toBeDisabled();
    }
    fireEvent.change(screen.getByLabelText("查询向量（JSON 数组）"), { target: { value: "[1,0]" } });
    fireEvent.change(screen.getByLabelText("Top K"), { target: { value: "201" } });
    expect(screen.getByRole("button", { name: "执行 KNN 查询" })).toBeDisabled();
  });
  it("能力或旧 schema 缺少元数据时不可执行", () => {
    const { rerender } = render(<VectorSearchPanel {...props} enabled={false} />);
    expect(screen.getByRole("button", { name: "执行 KNN 查询" })).toBeDisabled();
    rerender(<VectorSearchPanel {...props} attributes={[{ ...field, vector: undefined }]} />);
    expect(screen.getByText(/缺少可用的 FLOAT32/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "执行 KNN 查询" })).toBeDisabled();
  });
  it("修改输入或连接后隔离旧响应", async () => {
    let resolve!: (value: Awaited<ReturnType<typeof searchVectorIndex>>) => void;
    vi.mocked(searchVectorIndex).mockReturnValue(new Promise((done) => { resolve = done; }));
    const { rerender } = render(<VectorSearchPanel {...props} />);
    fireEvent.change(screen.getByLabelText("查询向量（JSON 数组）"), { target: { value: "[1,0]" } });
    fireEvent.click(screen.getByRole("button", { name: "执行 KNN 查询" }));
    rerender(<VectorSearchPanel {...props} connectionId="other" />);
    await act(async () => resolve({ matches: [{ key: "old:1", distance: 0 }], returned: 1, count: 5, distance_metric: "L2" }));
    await waitFor(() => expect(screen.getByRole("button", { name: "执行 KNN 查询" })).not.toHaveTextContent("查询中"));
    expect(screen.queryByText("old:1")).not.toBeInTheDocument();
  });
  it("查询期间修改向量会清空结果并忽略旧响应", async () => {
    let resolve!: (value: Awaited<ReturnType<typeof searchVectorIndex>>) => void;
    vi.mocked(searchVectorIndex).mockReturnValue(new Promise((done) => { resolve = done; }));
    render(<VectorSearchPanel {...props} />);
    fireEvent.change(screen.getByLabelText("查询向量（JSON 数组）"), { target: { value: "[1,0]" } });
    fireEvent.click(screen.getByRole("button", { name: "执行 KNN 查询" }));
    fireEvent.change(screen.getByLabelText("查询向量（JSON 数组）"), { target: { value: "[0,1]" } });
    await act(async () => resolve({ matches: [{ key: "old:vector", distance: 0 }], returned: 1, count: 5, distance_metric: "L2" }));
    expect(screen.queryByText("old:vector")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "执行 KNN 查询" })).toBeEnabled();
  });

});
