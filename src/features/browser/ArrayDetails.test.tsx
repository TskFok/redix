import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import ArrayDetails from "./ArrayDetails";

const { getArraySummaryMock, getArrayRangeMock, deleteArrayElementsMock, deleteArrayRangeMock } = vi.hoisted(() => ({
  getArraySummaryMock: vi.fn(),
  getArrayRangeMock: vi.fn(),
  deleteArrayElementsMock: vi.fn(),
  deleteArrayRangeMock: vi.fn(),
}));

vi.mock("../../lib/tauri", () => ({
  getArraySummary: getArraySummaryMock,
  getArrayRange: getArrayRangeMock,
  deleteArrayElements: deleteArrayElementsMock,
  deleteArrayRange: deleteArrayRangeMock,
}));

describe("ArrayDetails", () => {
  beforeEach(() => {
    vi.spyOn(window, "confirm").mockReturnValue(false);
    getArraySummaryMock.mockResolvedValue({ key: "events", length: "3", count: "2", next_index: "4" });
    getArrayRangeMock.mockResolvedValue({ start: "0", end: "499", has_more: false, cells: [{ index: "0", value: "created" }] });
    deleteArrayElementsMock.mockResolvedValue(1);
    deleteArrayRangeMock.mockResolvedValue(1);
  });
  afterEach(() => {
    cleanup();
    vi.clearAllMocks();
    vi.restoreAllMocks();
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

  it.each(["单项", "范围"])("%s删除等待确认，取消不请求，接受后仅发送一次", async (kind) => {
    render(<ArrayDetails connectionId="local" keyName="events" />);
    await screen.findByText("created");
    const remove = screen.getByRole("button", { name: kind === "单项" ? "删除" : "删除范围" });
    const request = kind === "单项" ? deleteArrayElementsMock : deleteArrayRangeMock;
    fireEvent.click(remove);
    expect(request).not.toHaveBeenCalled();
    fireEvent.click(within(screen.getByRole("alertdialog")).getByRole("button", { name: "取消" }));
    expect(request).not.toHaveBeenCalled();
    fireEvent.click(remove); fireEvent.click(remove);
    const accept = within(screen.getByRole("alertdialog")).getByRole("button", { name: "确认删除" });
    fireEvent.click(accept); fireEvent.click(accept);
    await waitFor(() => expect(request).toHaveBeenCalledTimes(1));
    expect(request).toHaveBeenCalledWith(kind === "单项"
      ? { connection_id: "local", key: "events", indices: ["0"] }
      : { connection_id: "local", key: "events", start: "0", end: "499" });
    await waitFor(() => expect(remove).toBeEnabled());
  });

  it.each(["键", "连接", "禁用", "范围", "标签"])("确认期间变更%s会取消旧删除", async (change) => {
    const view = render(<ArrayDetails connectionId="local" keyName="events" />);
    await screen.findByText("created");
    fireEvent.click(screen.getByRole("button", { name: "删除范围" }));
    expect(screen.getByRole("alertdialog")).toBeInTheDocument();
    if (change === "键") view.rerender(<ArrayDetails connectionId="local" keyName="other" />);
    else if (change === "连接") view.rerender(<ArrayDetails connectionId="remote" keyName="events" />);
    else if (change === "禁用") view.rerender(<ArrayDetails connectionId="local" keyName="events" disabled />);
    else if (change === "范围") fireEvent.change(screen.getByLabelText("起始索引"), { target: { value: "2" } });
    else fireEvent.click(screen.getByRole("tab", { name: "搜索" }));
    await waitFor(() => expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument());
    expect(deleteArrayRangeMock).not.toHaveBeenCalled();
  });

  it("接受确认后同一轮切换键不会向旧键发送请求", async () => {
    const view = render(<ArrayDetails connectionId="local" keyName="events" />);
    await screen.findByText("created");
    fireEvent.click(screen.getByRole("button", { name: "删除范围" }));
    act(() => {
      fireEvent.click(within(screen.getByRole("alertdialog")).getByRole("button", { name: "确认删除" }));
      view.rerender(<ArrayDetails connectionId="local" keyName="other" />);
    });
    await waitFor(() => expect(screen.getByRole("button", { name: "删除范围" })).toBeEnabled());
    expect(deleteArrayRangeMock).not.toHaveBeenCalled();
  });
});
