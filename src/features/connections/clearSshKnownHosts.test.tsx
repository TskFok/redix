import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { ConnectionProfile, SaveConnectionInput } from "../../lib/types";
import ConnectionForm from "./ConnectionForm";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
const invokeMock = vi.mocked(invoke);
const pickFileMock = vi.mocked(open);
const clearLabel = "清除全部已信任的 SSH 主机";

const profile: ConnectionProfile = {
  id: "remote", name: "远程 Redis", host: "127.0.0.1", port: 6379, username: null,
  database: 0, has_password: false, tls: false, verify_server_cert: true,
  ca_certificate_name: null, client_certificate_name: null, has_ca_certificate: false, has_client_certificate: false,
  ssh: { host: "bastion.example", port: 2222, username: "operator", auth_method: "private_key",
    has_password: false, has_private_key: false, has_passphrase: false, has_identity_file: true, has_known_hosts_file: true },
};

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => { resolve = done; });
  return { promise, resolve };
}

function approveClear() {
  fireEvent.click(within(screen.getByRole("alertdialog", { name: clearLabel })).getByRole("button", { name: "确认清除" }));
}

beforeEach(() => {
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (command, args) => {
    if (command === "save_connection") return (args as { input: SaveConnectionInput }).input.profile;
    if (command === "test_connection") return { server_version: "8.4.0" };
    return undefined;
  });
  pickFileMock.mockReset();
  pickFileMock.mockResolvedValue(null);
});

afterEach(cleanup);

describe("编辑连接时清除 SSH 主机信任", () => {
  it("仅在编辑连接且启用 SSH 时显示清除入口", () => {
    const view = render(<ConnectionForm onSaved={vi.fn()} onCancel={vi.fn()} />);
    fireEvent.click(screen.getByLabelText("启用 SSH 隧道"));
    expect(screen.queryByRole("button", { name: clearLabel })).not.toBeInTheDocument();
    view.unmount();

    render(<ConnectionForm initial={profile} onSaved={vi.fn()} onCancel={vi.fn()} />);
    expect(within(screen.getByRole("region", { name: "SSH 配置" })).getByRole("button", { name: clearLabel })).toBeInTheDocument();
    fireEvent.click(screen.getByLabelText("启用 SSH 隧道"));
    expect(screen.queryByRole("button", { name: clearLabel })).not.toBeInTheDocument();
  });

  it("确认后只清除应用全部信任记录，保留未保存草稿并阻止并行操作", async () => {
    const pending = deferred<void>();
    invokeMock.mockImplementation(() => pending.promise);
    const onSaved = vi.fn();
    render(<ConnectionForm initial={profile} onSaved={onSaved} onCancel={vi.fn()} />);
    fireEvent.change(screen.getByLabelText("连接名称"), { target: { value: "未保存的连接" } });
    fireEvent.change(screen.getByLabelText("SSH 端口"), { target: { value: "0" } });
    const save = screen.getByRole("button", { name: "保存" });
    const test = screen.getByRole("button", { name: "测试连接" });
    const file = screen.getByRole("button", { name: "选择 SSH 私钥文件" });

    act(() => {
      screen.getByRole("button", { name: clearLabel }).click();
      test.click();
      file.click();
      fireEvent.submit(save.closest("form")!);
    });
    const dialog = screen.getByRole("alertdialog", { name: clearLabel });
    expect(dialog).toHaveTextContent("应用内全部已信任的 SSH 主机");
    expect(dialog).toHaveTextContent("重新确认主机指纹");
    expect(dialog).toHaveTextContent("当前已建立的连接不受影响");
    expect(invokeMock).not.toHaveBeenCalled();
    expect(pickFileMock).not.toHaveBeenCalled();

    approveClear();
    await waitFor(() => expect(invokeMock).toHaveBeenCalledExactlyOnceWith("clear_ssh_known_hosts"));
    const clearing = screen.getByRole("button", { name: "清除中…" });
    for (const button of [clearing, save, test, file, screen.getByRole("button", { name: "保存并连接" })]) {
      expect(button).toBeDisabled();
      fireEvent.click(button);
    }
    fireEvent.submit(save.closest("form")!);
    expect(invokeMock).toHaveBeenCalledOnce();
    expect(pickFileMock).not.toHaveBeenCalled();

    await act(async () => { pending.resolve(); });
    expect(await screen.findByRole("status")).toHaveTextContent("已清除全部 SSH 主机信任记录");
    expect(screen.getByRole("button", { name: clearLabel })).toBeEnabled();
    expect(screen.getByLabelText("连接名称")).toHaveValue("未保存的连接");
    expect(screen.getByLabelText("SSH 端口")).toHaveValue(0);
    expect(onSaved).not.toHaveBeenCalled();
    expect(invokeMock).toHaveBeenCalledExactlyOnceWith("clear_ssh_known_hosts");
  });

  it.each(["取消", "Escape"])("通过%s退出确认时不清除信任记录", async (action) => {
    render(<ConnectionForm initial={profile} onSaved={vi.fn()} onCancel={vi.fn()} />);
    fireEvent.click(screen.getByRole("button", { name: clearLabel }));
    await act(async () => {
      if (action === "取消") fireEvent.click(within(screen.getByRole("alertdialog")).getByRole("button", { name: "取消" }));
      else fireEvent.keyDown(document, { key: "Escape" });
    });
    expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
    expect(invokeMock).not.toHaveBeenCalled();
    expect(screen.getByRole("button", { name: clearLabel })).toBeEnabled();
    expect(screen.getByRole("button", { name: "保存" })).toBeEnabled();
  });

  it.each(["卸载", "切换连接", "切换后返回原连接"])("等待确认时%s取消旧操作", async (change) => {
    const view = render(<ConnectionForm initial={profile} onSaved={vi.fn()} onCancel={vi.fn()} />);
    fireEvent.click(screen.getByRole("button", { name: clearLabel }));
    const approve = within(screen.getByRole("alertdialog")).getByRole("button", { name: "确认清除" });
    await act(async () => {
      if (change === "卸载") view.unmount();
      else view.rerender(<ConnectionForm initial={{ ...profile, id: "second" }} onSaved={vi.fn()} onCancel={vi.fn()} />);
    });
    if (change === "切换后返回原连接") view.rerender(<ConnectionForm initial={profile} onSaved={vi.fn()} onCancel={vi.fn()} />);
    await act(async () => { fireEvent.click(approve); });
    expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
    expect(invokeMock).not.toHaveBeenCalled();
  });

  it("确认后尚未提交时卸载也不会清除信任记录", async () => {
    const { unmount } = render(<ConnectionForm initial={profile} onSaved={vi.fn()} onCancel={vi.fn()} />);
    fireEvent.click(screen.getByRole("button", { name: clearLabel }));
    await act(async () => { approveClear(); unmount(); });
    expect(invokeMock).not.toHaveBeenCalled();
  });

  it("清除失败显示安全提示并允许重新确认后重试", async () => {
    invokeMock.mockRejectedValueOnce({ code: "SSH_HOST_KEY_STORE_FAILED", message: "/Users/private/known_hosts permission denied" });
    render(<ConnectionForm initial={profile} onSaved={vi.fn()} onCancel={vi.fn()} />);
    fireEvent.click(screen.getByRole("button", { name: clearLabel }));
    approveClear();
    expect(await screen.findByRole("alert")).toHaveTextContent("SSH 主机信任记录清除失败，请稍后重试。");
    expect(screen.getByRole("alert")).not.toHaveTextContent("private");
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: clearLabel })).toBeEnabled();
    fireEvent.click(screen.getByRole("button", { name: clearLabel }));
    expect(invokeMock).toHaveBeenCalledOnce();
    approveClear();
    expect(await screen.findByRole("status")).toHaveTextContent("已清除全部 SSH 主机信任记录");
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    expect(invokeMock).toHaveBeenCalledTimes(2);
  });

  it.each(["测试连接", "保存", "选择 SSH 私钥文件"])("%s期间禁止清除，包括同一批次点击", async (action) => {
    const pending = deferred<unknown>();
    if (action === "选择 SSH 私钥文件") pickFileMock.mockImplementation(() => pending.promise as Promise<null>);
    else invokeMock.mockImplementation(() => pending.promise);
    render(<ConnectionForm initial={profile} onSaved={vi.fn()} onCancel={vi.fn()} />);
    const clear = screen.getByRole("button", { name: clearLabel });
    act(() => {
      if (action === "保存") fireEvent.submit(screen.getByRole("button", { name: action }).closest("form")!);
      else screen.getByRole("button", { name: action }).click();
      clear.click();
    });
    expect(clear).toBeDisabled();
    expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
    expect(invokeMock.mock.calls.some(([command]) => command === "clear_ssh_known_hosts")).toBe(false);
    await act(async () => { pending.resolve(action === "保存" ? profile : action === "测试连接" ? { server_version: "8.4.0" } : null); });
    expect(clear).toBeEnabled();
  });
});
