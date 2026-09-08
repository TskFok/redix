import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import BrowserImportExport from "./BrowserImportExport";

const { exportKeys, importKeys } = vi.hoisted(() => ({ exportKeys: vi.fn(), importKeys: vi.fn() }));
vi.mock("../../lib/tauri", () => ({ exportKeys, importKeys }));

const entries = [{ key: "user:1", ttl_ms: -1, value: { String: { value: "Alice" } } }];

beforeEach(() => {
  vi.useFakeTimers();
  exportKeys.mockResolvedValue(entries);
  importKeys.mockResolvedValue(1);
  vi.stubGlobal("URL", { createObjectURL: () => "blob:keys", revokeObjectURL: vi.fn() });
  vi.spyOn(HTMLAnchorElement.prototype, "click").mockImplementation(() => {});
});

afterEach(() => {
  cleanup();
  vi.useRealTimers();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

function renderActions() {
  return render(<BrowserImportExport connectionId="local" selectedKeys={["user:1"]} onImported={async () => {}} />);
}

async function exportSelected() {
  await act(async () => { fireEvent.click(screen.getByRole("button", { name: "导出选中键" })); });
}

describe("键导入导出操作反馈", () => {
  it("导出成功提示显示 3 秒后自动消失", async () => {
    renderActions();
    await exportSelected();
    expect(screen.getByRole("status")).toHaveTextContent("已导出 1 个键。");
    act(() => { vi.advanceTimersByTime(2999); });
    expect(screen.getByRole("status")).toBeInTheDocument();
    act(() => { vi.advanceTimersByTime(1); });
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
  });

  it("再次导出相同数量的键后重新计时", async () => {
    renderActions();
    await exportSelected();
    act(() => { vi.advanceTimersByTime(2000); });
    await exportSelected();
    act(() => { vi.advanceTimersByTime(1000); });
    expect(screen.getByRole("status")).toHaveTextContent("已导出 1 个键。");
    act(() => { vi.advanceTimersByTime(2000); });
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
  });

  it("导入完成反馈自动消失", async () => {
    renderActions();
    const file = new File([JSON.stringify(entries)], "keys.json", { type: "application/json" });
    Object.defineProperty(file, "text", { value: async () => JSON.stringify(entries) });
    await act(async () => {
      fireEvent.change(screen.getByLabelText("导入 JSON 文件"), { target: { files: [file] } });
    });
    expect(screen.getByRole("status")).toHaveTextContent("已导入 1 个键");
    act(() => { vi.advanceTimersByTime(3000); });
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
  });

  it("导出失败提示显示 3 秒后自动消失", async () => {
    exportKeys.mockRejectedValueOnce({ code: "COMMAND_FAILED" });
    renderActions();
    await exportSelected();
    expect(screen.getByRole("alert")).toBeInTheDocument();
    act(() => { vi.advanceTimersByTime(3000); });
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
  });
});
