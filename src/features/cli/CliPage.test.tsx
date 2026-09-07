import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { StrictMode } from "react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import CliPage from "./CliPage";
const api = vi.hoisted(() => ({ openCliSession: vi.fn(), executeCliCommand: vi.fn(), closeCliSession: vi.fn() }));
vi.mock("./cliApi", () => api);
beforeEach(() => {
  vi.resetAllMocks();
  api.openCliSession.mockResolvedValue(undefined);
  api.closeCliSession.mockResolvedValue(undefined);
  api.executeCliCommand.mockResolvedValue({ result: { kind: "string", value: "OK" }, error_code: null, session_closed: false, truncated: false });
});
afterEach(cleanup);

it("Cluster CLI 说明普通键槽路由并明确禁用连接状态命令", async () => {
  render(<CliPage connectionId="cluster" database={0} isCluster />);

  expect(await screen.findByText(/Cluster 普通命令按键槽路由/)).toBeInTheDocument();
  expect(screen.getByText(/不支持 MULTI\/EXEC、WATCH、SELECT/)).toBeInTheDocument();
  expect(screen.getByText(/订阅与 MONITOR 在 Cluster 中不支持/)).toBeInTheDocument();
});

it("嵌套 RESP 结果格式化膨胀后受单项输出限制且不会清空旧历史", async () => {
  let nested: unknown = Array.from({ length: 20_000 }, () => 1);
  for (let index = 0; index < 60; index += 1) nested = [nested];
  expect(new TextEncoder().encode(JSON.stringify(nested)).byteLength).toBeLessThan(256 * 1024);
  api.executeCliCommand
    .mockResolvedValueOnce({ result: { kind: "string", value: "PONG" }, error_code: null, session_closed: false, truncated: false })
    .mockResolvedValueOnce({ result: { kind: "array", value: nested }, error_code: null, session_closed: false, truncated: false });
  render(<CliPage connectionId="local" database={0} />);
  const input = screen.getByRole("textbox", { name: "CLI 命令" });
  await waitFor(() => expect(input).toBeEnabled());
  for (const command of ["PING", "EVAL nested 0"]) {
    fireEvent.change(input, { target: { value: command } });
    fireEvent.keyDown(input, { key: "Enter" });
    await waitFor(() => expect(input).toBeEnabled());
  }
  expect(screen.getByText("PONG")).toBeInTheDocument();
  expect(screen.getByText(/输出超过 256 KiB，已省略/)).toBeInTheDocument();
  for (const output of screen.getByRole("log", { name: "CLI 输出" }).querySelectorAll("pre")) {
    expect(new TextEncoder().encode(output.textContent ?? "").byteLength).toBeLessThanOrEqual(256 * 1024);
  }
});

it("StrictMode 重建后迟到的旧连接不会关闭当前会话", async () => {
  const pending: (() => void)[] = [];
  const live = new Set<string>();
  api.openCliSession.mockImplementation((session) => new Promise<void>((resolve) => {
    live.add(session.session_id);
    pending.push(resolve);
  }));
  api.closeCliSession.mockImplementation(async (session) => { live.delete(session.session_id); });
  api.executeCliCommand.mockImplementation(async (session) => {
    if (!live.has(session.session_id)) throw { code: "CONNECTION_FAILED" };
    return { result: { kind: "string", value: "PONG" }, error_code: null, session_closed: false, truncated: false };
  });
  render(<StrictMode><CliPage connectionId="local" database={0} /></StrictMode>);
  await act(async () => pending[1]());
  await act(async () => pending[0]());
  const input = screen.getByRole("textbox", { name: "CLI 命令" });
  fireEvent.change(input, { target: { value: "PING" } });
  fireEvent.keyDown(input, { key: "Enter" });
  await screen.findByText("PONG");
  expect(input).toBeEnabled();
});

it("连接中也能关闭，迟到的连接结果不能重新启用会话", async () => {
  let finish: (() => void) | undefined;
  api.openCliSession.mockImplementation(() => new Promise<void>((resolve) => { finish = resolve; }));
  render(<CliPage connectionId="local" database={0} />);
  fireEvent.click(screen.getByRole("button", { name: "关闭 CLI" }));
  expect(screen.getByRole("button", { name: "重新连接 CLI" })).toBeInTheDocument();
  await act(async () => finish?.());
  expect(screen.getByRole("textbox", { name: "CLI 命令" })).toBeDisabled();
  expect(screen.getByRole("button", { name: "重新连接 CLI" })).toBeInTheDocument();
});

it("多轮命令使用同一会话且卸载时关闭，不保存历史", async () => {
  const persist = vi.spyOn(Storage.prototype, "setItem");
  const view = render(<CliPage connectionId="local" database={0} />);
  const input = screen.getByRole("textbox", { name: "CLI 命令" });
  await waitFor(() => expect(input).toBeEnabled());
  for (const command of ["MULTI", "SET key value", "EXEC"]) {
    fireEvent.change(input, { target: { value: command } });
    fireEvent.keyDown(input, { key: "Enter" });
    await waitFor(() => expect(input).toBeEnabled());
  }
  const session = api.openCliSession.mock.calls[0][0];
  expect(api.executeCliCommand.mock.calls.map(([input]) => input.session_id)).toEqual([session.session_id, session.session_id, session.session_id]);
  expect(screen.getByText(/不自动保存/)).toBeInTheDocument();
  view.unmount();
  expect(api.closeCliSession).toHaveBeenCalledWith(session);
  expect(persist).not.toHaveBeenCalled();
  persist.mockRestore();
});

it("执行期间重复回车不会重复发出命令", async () => {
  let finish: ((reply: unknown) => void) | undefined;
  api.executeCliCommand.mockImplementation(() => new Promise((resolve) => { finish = resolve; }));
  render(<CliPage connectionId="local" database={0} />);
  const input = screen.getByRole("textbox", { name: "CLI 命令" });
  await waitFor(() => expect(input).toBeEnabled());
  fireEvent.change(input, { target: { value: "INCR counter" } });
  fireEvent.keyDown(input, { key: "Enter" });
  fireEvent.keyDown(input, { key: "Enter" });
  expect(api.executeCliCommand).toHaveBeenCalledTimes(1);
  expect(input).toBeDisabled();
  await act(async () => finish?.({ result: { kind: "integer", value: 1 }, error_code: null, session_closed: false, truncated: false }));
  expect(screen.getByText("1")).toBeInTheDocument();
  expect(input).toBeEnabled();
});

it("超时后禁用旧会话且显式重新连接使用新会话", async () => {
  api.executeCliCommand.mockResolvedValue({ result: null, error_code: "COMMAND_TIMEOUT", session_closed: true, truncated: false });
  render(<CliPage connectionId="local" database={0} />);
  const input = screen.getByRole("textbox", { name: "CLI 命令" });
  await waitFor(() => expect(input).toBeEnabled());
  fireEvent.change(input, { target: { value: "BLPOP missing 0" } });
  fireEvent.keyDown(input, { key: "Enter" });
  await screen.findByText(/COMMAND_TIMEOUT/);
  expect(input).toBeDisabled();
  fireEvent.click(screen.getByRole("button", { name: "重新连接 CLI" }));
  await waitFor(() => expect(screen.getByRole("textbox", { name: "CLI 命令" })).toBeEnabled());
  expect(api.openCliSession.mock.calls[0][0].session_id).not.toBe(api.openCliSession.mock.calls[1][0].session_id);
});

it("切换数据库后旧命令返回不会出现在新会话", async () => {
  let finish: ((reply: unknown) => void) | undefined;
  api.executeCliCommand.mockImplementation(() => new Promise((resolve) => { finish = resolve; }));
  const view = render(<CliPage connectionId="local" database={0} />);
  const input = screen.getByRole("textbox", { name: "CLI 命令" });
  await waitFor(() => expect(input).toBeEnabled());
  fireEvent.change(input, { target: { value: "GET old" } });
  fireEvent.keyDown(input, { key: "Enter" });
  view.rerender(<CliPage connectionId="local" database={1} />);
  await act(async () => finish?.({ result: { kind: "string", value: "old-secret" }, error_code: null, session_closed: false, truncated: false }));
  expect(screen.queryByText("old-secret")).not.toBeInTheDocument();
  expect(api.closeCliSession).toHaveBeenCalledWith(api.openCliSession.mock.calls[0][0]);
});
