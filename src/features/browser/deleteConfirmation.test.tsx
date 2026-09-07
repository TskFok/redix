import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import BulkKeyActions from "./BulkKeyActions";
import StartBulkDeleteButton from "../tasks/StartBulkDeleteButton";
import { BULK_TASKS_CHANGED } from "../tasks/bulkTaskApi";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const api = vi.mocked(invoke);

beforeEach(() => {
  api.mockReset();
  vi.stubGlobal("confirm", vi.fn(() => false));
});
afterEach(() => { cleanup(); vi.unstubAllGlobals(); });

describe.each([
  { kind: "同步", label: "批量删除（2）", command: "delete_keys" },
  { kind: "后台", label: "后台批量删除", command: "start_bulk_delete" },
])("$kind 批量删除确认", ({ kind, label, command }) => {
  const onDeleted = vi.fn();
  const onError = vi.fn();
  const view = (connectionId = "one", keys = ["user:1", "user:2"], disabled = false) => kind === "同步"
    ? <BulkKeyActions connectionId={connectionId} selectedKeys={keys} busy={disabled} onDeleted={onDeleted} onError={onError} />
    : <StartBulkDeleteButton connectionId={connectionId} keys={keys} disabled={disabled} />;

  beforeEach(() => { onDeleted.mockClear(); onError.mockClear(); });

  it("浏览器原生确认不可用时仍显示确认框，确认后只提交一次选中的键", async () => {
    let finish!: (value: number) => void;
    api.mockImplementation(() => new Promise((resolve) => { finish = resolve; }));
    const changed = vi.fn();
    window.addEventListener(BULK_TASKS_CHANGED, changed);
    try {
      render(view());
      fireEvent.click(screen.getByRole("button", { name: label }));
      const dialog = screen.getByRole("alertdialog", { name: "确认删除" });
      expect(dialog).toHaveTextContent("2 个键");
      expect(api).not.toHaveBeenCalled();
      fireEvent.click(within(dialog).getByRole("button", { name: "确认删除" }));
      await waitFor(() => expect(api).toHaveBeenCalledExactlyOnceWith(command, {
        input: { connection_id: "one", keys: ["user:1", "user:2"] },
      }));
      const submitting = screen.getByRole("button", { name: kind === "同步" ? "删除中…" : "启动中…" });
      expect(submitting).toBeDisabled();
      fireEvent.click(submitting);
      expect(api).toHaveBeenCalledOnce();
      finish(2);
      await waitFor(() => expect(screen.getByRole("button", { name: label })).toBeEnabled());
      if (kind === "同步") expect(onDeleted).toHaveBeenCalledExactlyOnceWith(2);
      else expect(changed).toHaveBeenCalledOnce();
    } finally {
      window.removeEventListener(BULK_TASKS_CHANGED, changed);
    }
  });

  it("取消确认保留选择且不提交删除", async () => {
    render(view());
    fireEvent.click(screen.getByRole("button", { name: label }));
    await act(async () => {
      fireEvent.click(within(screen.getByRole("alertdialog")).getByRole("button", { name: "取消" }));
    });
    expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
    expect(api).not.toHaveBeenCalled();
    expect(screen.getByRole("button", { name: label })).toBeEnabled();
  });

  it.each(["连接", "选中键", "禁用"])("等待确认时%s变化会取消旧操作", async (change) => {
    const { rerender } = render(view());
    fireEvent.click(screen.getByRole("button", { name: label }));
    const approve = within(screen.getByRole("alertdialog")).getByRole("button", { name: "确认删除" });
    rerender(view(change === "连接" ? "two" : "one", change === "选中键" ? ["other"] : ["user:1", "user:2"], change === "禁用"));
    expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
    await act(async () => { fireEvent.click(approve); });
    expect(api).not.toHaveBeenCalled();
  });

  it("删除接口失败后反馈错误并恢复操作按钮", async () => {
    api.mockRejectedValue({ code: "COMMAND_FAILED", message: "command failed" });
    render(view());
    fireEvent.click(screen.getByRole("button", { name: label }));
    fireEvent.click(within(screen.getByRole("alertdialog")).getByRole("button", { name: "确认删除" }));
    if (kind === "同步") await waitFor(() => expect(onError).toHaveBeenCalledWith("批量删除失败，请稍后重试。"));
    else expect(await screen.findByRole("alert")).toHaveTextContent("后台删除启动失败");
    expect(screen.getByRole("button", { name: label })).toBeEnabled();
  });
});
