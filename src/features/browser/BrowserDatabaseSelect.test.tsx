import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { getDatabaseOverview, selectDatabase } from "../../lib/tauri";
import type { ConnectionProfile, DatabaseOverview } from "../../lib/types";
import BrowserDatabaseSelect from "./BrowserDatabaseSelect";

vi.mock("../../lib/tauri", () => ({ getDatabaseOverview: vi.fn(), selectDatabase: vi.fn() }));

const getDatabaseOverviewMock = vi.mocked(getDatabaseOverview);
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
  getDatabaseOverviewMock.mockImplementation(() => new Promise(() => {}));
  selectDatabaseMock.mockImplementation(async ({ database }) => ({ ...profile, database }));
});
afterEach(cleanup);

describe("BrowserDatabaseSelect", () => {
  it("显示各数据库的 key 总数、空库零值，并保留未知数量", async () => {
    getDatabaseOverviewMock.mockResolvedValue([
      { database: 0, key_count: 1234, expires: 20, avg_ttl_ms: 1000 },
      { database: 1, key_count: 0, expires: 0, avg_ttl_ms: 0 },
      { database: 2, key_count: null, expires: null, avg_ttl_ms: null },
    ]);
    render(<BrowserDatabaseSelect {...defaultProps()} />);

    expect(await screen.findByRole("option", { name: "DB 0（1,234 keys）" })).toHaveValue("0");
    expect(screen.getByRole("option", { name: "DB 1（0 keys）" })).toHaveValue("1");
    expect(screen.getByRole("option", { name: "DB 2（—）" })).toHaveValue("2");
    expect(screen.getByRole("option", { name: "DB 15（—）" })).toHaveValue("15");
    expect(getDatabaseOverviewMock).toHaveBeenCalledWith("local");
    expect(selectDatabaseMock).not.toHaveBeenCalled();
  });

  it("统计失败不会把未知库当作空库，也不阻止切库", async () => {
    getDatabaseOverviewMock.mockRejectedValue(new Error("private server details"));
    const props = defaultProps();
    render(<BrowserDatabaseSelect {...props} />);
    await act(async () => {});

    const select = screen.getByRole("combobox", { name: "切换数据库" });
    expect(select).toBeEnabled();
    expect(screen.getByRole("option", { name: "DB 0（—）" })).toBeInTheDocument();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    fireEvent.change(select, { target: { value: "15" } });
    await waitFor(() => expect(props.onProfileChanged).toHaveBeenCalledWith({ ...profile, database: 15 }));
  });

  it("刷新后更新数量，失败后清除旧统计并支持恢复", async () => {
    const props = defaultProps();
    getDatabaseOverviewMock.mockResolvedValueOnce([
      { database: 0, key_count: 4, expires: 0, avg_ttl_ms: 0 },
    ]).mockRejectedValueOnce(new Error("unavailable")).mockResolvedValueOnce([
      { database: 0, key_count: 9, expires: 0, avg_ttl_ms: 0 },
    ]);
    const view = render(<BrowserDatabaseSelect {...props} refreshToken={0} />);
    await screen.findByRole("option", { name: "DB 0（4 keys）" });
    view.rerender(<BrowserDatabaseSelect {...props} refreshToken={1} />);
    await screen.findByRole("option", { name: "DB 0（—）" });
    view.rerender(<BrowserDatabaseSelect {...props} refreshToken={2} />);
    await screen.findByRole("option", { name: "DB 0（9 keys）" });
  });

  it.each(["success", "failure"])("更换连接后忽略旧统计请求的 %s 结果", async (result) => {
    let resolve!: (value: DatabaseOverview[]) => void;
    let reject!: (reason: unknown) => void;
    getDatabaseOverviewMock.mockReturnValueOnce(new Promise((success, failure) => {
      resolve = success;
      reject = failure;
    })).mockResolvedValueOnce([
      { database: 0, key_count: 8, expires: 0, avg_ttl_ms: 0 },
    ]);
    const props = defaultProps();
    const view = render(<BrowserDatabaseSelect {...props} />);
    view.rerender(<BrowserDatabaseSelect {...props} connectionId="next" />);
    await screen.findByRole("option", { name: "DB 0（8 keys）" });
    await act(async () => {
      if (result === "success") resolve([{ database: 0, key_count: 99, expires: 0, avg_ttl_ms: 0 }]);
      else reject(new Error("old connection failed"));
    });
    expect(screen.getByRole("option", { name: "DB 0（8 keys）" })).toBeInTheDocument();
  });

  it("切换当前库时重新加载统计，保留 fallback 以外库的未知状态", async () => {
    getDatabaseOverviewMock.mockResolvedValueOnce([
      { database: 0, key_count: 2, expires: null, avg_ttl_ms: null },
    ]).mockResolvedValueOnce([
      { database: 1, key_count: 6, expires: null, avg_ttl_ms: null },
    ]);
    const props = defaultProps();
    const view = render(<BrowserDatabaseSelect {...props} />);
    await screen.findByRole("option", { name: "DB 0（2 keys）" });
    view.rerender(<BrowserDatabaseSelect {...props} activeDatabase={1} />);
    await screen.findByRole("option", { name: "DB 1（6 keys）" });
    expect(screen.getByRole("option", { name: "DB 0（—）" })).toBeInTheDocument();
  });

  it("Cluster 显示 DB 0 的汇总 key 数量并保持禁止切库", async () => {
    getDatabaseOverviewMock.mockResolvedValue([
      { database: 0, key_count: 2500, expires: null, avg_ttl_ms: null },
    ]);
    render(<BrowserDatabaseSelect {...defaultProps()} isCluster />);
    await screen.findByRole("option", { name: "DB 0（2,500 keys）" });
    expect(screen.getAllByRole("option")).toHaveLength(1);
    expect(screen.getByRole("combobox", { name: "切换数据库" })).toBeDisabled();
  });

  it("允许直接切换到空数据库，成功后通过 profile 回调同步当前 Db", async () => {
    const props = defaultProps();
    const pending = pendingSelection();
    selectDatabaseMock.mockReturnValueOnce(pending.promise);
    const view = render(<BrowserDatabaseSelect {...props} />);
    const select = screen.getByRole("combobox", { name: "切换数据库" });

    expect(screen.getAllByRole("option")).toHaveLength(16);
    expect(screen.getByRole("option", { name: /^DB 15/ })).toHaveValue("15");
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
