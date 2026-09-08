import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { selectDatabase } from "../../lib/tauri";
import type { ConnectionProfile } from "../../lib/types";
import BrowserDatabaseSelect from "./BrowserDatabaseSelect";

vi.mock("../../lib/tauri", () => ({ selectDatabase: vi.fn() }));

const selectDatabaseMock = vi.mocked(selectDatabase);
const profile: ConnectionProfile = {
  id: "local", name: "本地 Redis", host: "127.0.0.1", port: 6379,
  username: null, database: 0, has_password: false, tls: false,
  verify_server_cert: true, ca_certificate_name: null, client_certificate_name: null,
  has_ca_certificate: false, has_client_certificate: false,
};

function pendingSelection() {
  let resolve!: (value: ConnectionProfile) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<ConnectionProfile>((success, failure) => {
    resolve = success;
    reject = failure;
  });
  return { promise, resolve, reject };
}

function defaultProps() {
  return {
    connectionId: "local", activeDatabase: 0,
    onProfileChanged: vi.fn(), onSwitchingChange: vi.fn(),
  };
}

beforeEach(() => {
  vi.resetAllMocks();
  selectDatabaseMock.mockImplementation(async ({ database }) => ({ ...profile, database }));
});
afterEach(cleanup);

describe("BrowserDatabaseSelect", () => {
  it("允许直接切换到空数据库，成功后通过 profile 回调同步当前 Db", async () => {
    const props = defaultProps();
    const pending = pendingSelection();
    selectDatabaseMock.mockReturnValueOnce(pending.promise);
    const view = render(<BrowserDatabaseSelect {...props} />);
    const select = screen.getByRole("combobox", { name: "切换数据库" });

    expect(screen.getAllByRole("option")).toHaveLength(16);
    expect(screen.getByRole("option", { name: "DB 15" })).toHaveValue("15");
    expect(select).toHaveValue("0");
    fireEvent.change(select, { target: { value: "15" } });

    expect(selectDatabaseMock).toHaveBeenCalledWith({ connection_id: "local", database: 15 });
    expect(select).toBeDisabled();
    expect(select).toHaveValue("0");
    expect(props.onSwitchingChange).toHaveBeenLastCalledWith(true);
    expect(props.onProfileChanged).not.toHaveBeenCalled();

    await act(async () => { pending.resolve({ ...profile, database: 15 }); });
    expect(props.onProfileChanged).toHaveBeenCalledWith({ ...profile, database: 15 });
    expect(props.onSwitchingChange).toHaveBeenLastCalledWith(false);
    view.rerender(<BrowserDatabaseSelect {...props} activeDatabase={15} />);
    expect(select).toHaveValue("15");
    expect(select).toBeEnabled();
  });

  it("切换失败时保留当前数据库，显示固定提示并恢复操作", async () => {
    const props = defaultProps();
    selectDatabaseMock.mockRejectedValueOnce({ code: "COMMAND_FAILED", message: "private server details" });
    render(<BrowserDatabaseSelect {...props} activeDatabase={3} />);
    const select = screen.getByRole("combobox", { name: "切换数据库" });
    fireEvent.change(select, { target: { value: "4" } });

    expect(await screen.findByRole("alert")).toHaveTextContent("数据库切换失败");
    expect(screen.getByRole("alert")).toHaveTextContent("保留当前 Db");
    expect(screen.queryByText("private server details")).not.toBeInTheDocument();
    expect(select).toHaveValue("3");
    expect(select).toBeEnabled();
    expect(props.onProfileChanged).not.toHaveBeenCalled();
    expect(props.onSwitchingChange.mock.calls).toEqual([[true], [false]]);
  });

  it("相同数据库和进行中的重复选择不会再次提交", async () => {
    const props = defaultProps();
    const pending = pendingSelection();
    selectDatabaseMock.mockReturnValueOnce(pending.promise);
    render(<BrowserDatabaseSelect {...props} />);
    const select = screen.getByRole("combobox", { name: "切换数据库" });

    fireEvent.change(select, { target: { value: "0" } });
    expect(selectDatabaseMock).not.toHaveBeenCalled();
    fireEvent.change(select, { target: { value: "1" } });
    fireEvent.change(select, { target: { value: "2" } });
    expect(selectDatabaseMock).toHaveBeenCalledTimes(1);
    expect(props.onSwitchingChange.mock.calls).toEqual([[true]]);
    await act(async () => { pending.resolve({ ...profile, database: 1 }); });
  });

  it("Cluster 只显示 DB 0 并禁用切库", () => {
    const props = defaultProps();
    render(<BrowserDatabaseSelect {...props} isCluster />);
    const select = screen.getByRole("combobox", { name: "切换数据库" });
    expect(select).toBeDisabled();
    expect(select).toHaveAttribute("title", "Cluster 仅支持 DB 0");
    expect(screen.getAllByRole("option")).toHaveLength(1);
    fireEvent.change(select, { target: { value: "1" } });
    expect(selectDatabaseMock).not.toHaveBeenCalled();
  });

  it("父页面忙碌时不提交切库", () => {
    const props = defaultProps();
    render(<BrowserDatabaseSelect {...props} disabled />);
    const select = screen.getByRole("combobox", { name: "切换数据库" });
    expect(select).toBeDisabled();
    fireEvent.change(select, { target: { value: "1" } });
    expect(selectDatabaseMock).not.toHaveBeenCalled();
    expect(props.onSwitchingChange).not.toHaveBeenCalled();
  });

  it("卸载后成功仍同步后台 profile 并释放原连接的切换状态", async () => {
    const props = defaultProps();
    const pending = pendingSelection();
    selectDatabaseMock.mockReturnValueOnce(pending.promise);
    const view = render(<BrowserDatabaseSelect {...props} />);
    fireEvent.change(screen.getByRole("combobox", { name: "切换数据库" }), { target: { value: "1" } });
    view.unmount();
    await act(async () => { pending.resolve({ ...profile, database: 1 }); });

    expect(props.onProfileChanged).toHaveBeenCalledWith({ ...profile, database: 1 });
    expect(props.onSwitchingChange.mock.calls).toEqual([[true], [false]]);
  });

  it("成功回调使页面卸载后仍通知原连接切换完成", async () => {
    const props = defaultProps();
    const pending = pendingSelection();
    selectDatabaseMock.mockReturnValueOnce(pending.promise);
    const view = render(<BrowserDatabaseSelect {...props} />);
    props.onProfileChanged.mockImplementation(() => view.unmount());
    fireEvent.change(screen.getByRole("combobox", { name: "切换数据库" }), { target: { value: "1" } });
    await act(async () => { pending.resolve({ ...profile, database: 1 }); });

    expect(props.onSwitchingChange.mock.calls).toEqual([[true], [false]]);
  });

  it.each(["success", "failure"])("更换连接后旧请求 %s 不覆盖新连接进行中的切换", async (result) => {
    const oldProps = defaultProps();
    const newProps = { ...defaultProps(), connectionId: "next", activeDatabase: 3 };
    const oldPending = pendingSelection();
    const newPending = pendingSelection();
    selectDatabaseMock.mockReturnValueOnce(oldPending.promise).mockReturnValueOnce(newPending.promise);
    const view = render(<BrowserDatabaseSelect {...oldProps} />);
    fireEvent.change(screen.getByRole("combobox", { name: "切换数据库" }), { target: { value: "1" } });
    view.rerender(<BrowserDatabaseSelect {...newProps} />);
    const select = screen.getByRole("combobox", { name: "切换数据库" });
    expect(select).toHaveValue("3");
    expect(select).toBeEnabled();
    fireEvent.change(select, { target: { value: "4" } });
    const newSwitchingCalls = [...newProps.onSwitchingChange.mock.calls];

    await act(async () => {
      if (result === "success") oldPending.resolve({ ...profile, database: 1 });
      else oldPending.reject({ code: "COMMAND_FAILED" });
    });
    expect(select).toBeDisabled();
    expect(select).toHaveValue("3");
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    expect(newProps.onSwitchingChange.mock.calls).toEqual(newSwitchingCalls);
    if (result === "success") expect(oldProps.onProfileChanged).toHaveBeenCalledWith({ ...profile, database: 1 });
    else expect(oldProps.onProfileChanged).not.toHaveBeenCalled();
    expect(newProps.onProfileChanged).not.toHaveBeenCalled();

    await act(async () => { newPending.resolve({ ...profile, id: "next", database: 4 }); });
    await waitFor(() => expect(select).toBeEnabled());
    expect(newProps.onProfileChanged).toHaveBeenCalledWith({ ...profile, id: "next", database: 4 });
  });
});
