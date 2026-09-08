import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import VectorSetDetails from "./VectorSetDetails";

const {
  getVectorSetSummaryMock,
  listVectorSetElementsMock,
  getVectorSetElementMock,
  deleteVectorSetElementsMock,
  deleteVectorSetAttributesMock,
} = vi.hoisted(() => ({
  getVectorSetSummaryMock: vi.fn(),
  listVectorSetElementsMock: vi.fn(),
  getVectorSetElementMock: vi.fn(),
  deleteVectorSetElementsMock: vi.fn(),
  deleteVectorSetAttributesMock: vi.fn(),
}));

vi.mock("../../lib/tauri", () => ({
  getVectorSetSummary: getVectorSetSummaryMock,
  listVectorSetElements: listVectorSetElementsMock,
  getVectorSetElement: getVectorSetElementMock,
  deleteVectorSetElements: deleteVectorSetElementsMock,
  deleteVectorSetAttributes: deleteVectorSetAttributesMock,
}));

describe("VectorSetDetails", () => {
  beforeEach(() => {
    vi.spyOn(window, "confirm").mockReturnValue(false);
    getVectorSetSummaryMock.mockResolvedValue({ key: "embeddings", total: "2", dimension: 3, quantization: "f32" });
    listVectorSetElementsMock.mockResolvedValue({ cursor: null, has_more: false, elements: [
      { name: "one", score: null, vector_base64: null, attributes: { label: "a" } },
      { name: "two", score: null, vector_base64: null, attributes: { label: "b" } },
    ] });
    getVectorSetElementMock.mockImplementation(({ element }: { element: string }) => Promise.resolve({ name: element, score: null, vector_base64: null, attributes: { label: element } }));
    deleteVectorSetElementsMock.mockResolvedValue(1);
    deleteVectorSetAttributesMock.mockResolvedValue(undefined);
  });
  afterEach(() => {
    cleanup();
    vi.clearAllMocks();
    vi.restoreAllMocks();
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

  it.each(["元素", "属性"])("删除%s等待确认，取消不请求，接受后仅提交一次", async (kind) => {
    render(<VectorSetDetails connectionId="local" keyName="embeddings" />);
    await screen.findByText("one");
    if (kind === "元素") fireEvent.click(screen.getByLabelText("选择元素 one"));
    else {
      fireEvent.click(screen.getAllByRole("button", { name: "查看" })[0]);
      await screen.findByText("元素详情：one");
    }
    const remove = screen.getByRole("button", { name: kind === "元素" ? "删除选中" : "清除属性" });
    const request = kind === "元素" ? deleteVectorSetElementsMock : deleteVectorSetAttributesMock;
    fireEvent.click(remove);
    expect(request).not.toHaveBeenCalled();
    fireEvent.click(within(screen.getByRole("alertdialog")).getByRole("button", { name: "取消" }));
    expect(request).not.toHaveBeenCalled();
    fireEvent.click(remove); fireEvent.click(remove);
    const accept = within(screen.getByRole("alertdialog")).getByRole("button", { name: kind === "元素" ? "确认删除" : "确认清除" });
    fireEvent.click(accept); fireEvent.click(accept);
    await waitFor(() => expect(request).toHaveBeenCalledTimes(1));
    expect(request).toHaveBeenCalledWith(kind === "元素"
      ? { connection_id: "local", key: "embeddings", elements: ["one"] }
      : { connection_id: "local", key: "embeddings", element: "one" });
    if (kind === "属性") await waitFor(() => expect(screen.getByLabelText("属性 JSON")).toHaveValue("null"));
    else await waitFor(() => expect(screen.getByLabelText("选择元素 one")).not.toBeChecked());
  });

  it.each(["键", "连接", "禁用", "选择", "标签"])("确认期间变更%s会取消旧批量删除", async (change) => {
    const view = render(<VectorSetDetails connectionId="local" keyName="embeddings" />);
    await screen.findByText("one");
    fireEvent.click(screen.getByLabelText("选择元素 one"));
    fireEvent.click(screen.getByRole("button", { name: "删除选中" }));
    expect(screen.getByRole("alertdialog")).toBeInTheDocument();
    if (change === "键") view.rerender(<VectorSetDetails connectionId="local" keyName="other" />);
    else if (change === "连接") view.rerender(<VectorSetDetails connectionId="remote" keyName="embeddings" />);
    else if (change === "禁用") view.rerender(<VectorSetDetails connectionId="local" keyName="embeddings" disabled />);
    else if (change === "选择") fireEvent.click(screen.getByLabelText("选择元素 two"));
    else fireEvent.click(screen.getByRole("tab", { name: "相似度搜索" }));
    await waitFor(() => expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument());
    expect(deleteVectorSetElementsMock).not.toHaveBeenCalled();
  });

  it.each(["关闭", "属性", "元素"])("清除属性确认期间变更%s会取消旧确认", async (change) => {
    render(<VectorSetDetails connectionId="local" keyName="embeddings" />);
    await screen.findByText("one");
    fireEvent.click(screen.getAllByRole("button", { name: "查看" })[0]);
    await screen.findByText("元素详情：one");
    fireEvent.click(screen.getByRole("button", { name: "清除属性" }));
    expect(screen.getByRole("alertdialog")).toBeInTheDocument();
    if (change === "关闭") fireEvent.click(screen.getByRole("button", { name: "关闭" }));
    else if (change === "属性") fireEvent.change(screen.getByLabelText("属性 JSON"), { target: { value: '{"new":true}' } });
    else fireEvent.click(screen.getAllByRole("button", { name: "查看" })[1]);
    await waitFor(() => expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument());
    expect(deleteVectorSetAttributesMock).not.toHaveBeenCalled();
  });

  it("接受确认后同一轮变更选择不会提交旧批量删除", async () => {
    render(<VectorSetDetails connectionId="local" keyName="embeddings" />);
    await screen.findByText("one");
    fireEvent.click(screen.getByLabelText("选择元素 one"));
    fireEvent.click(screen.getByRole("button", { name: "删除选中" }));
    act(() => {
      fireEvent.click(within(screen.getByRole("alertdialog")).getByRole("button", { name: "确认删除" }));
      fireEvent.click(screen.getByLabelText("选择元素 two"));
    });
    await act(async () => {});
    expect(deleteVectorSetElementsMock).not.toHaveBeenCalled();
  });
});
