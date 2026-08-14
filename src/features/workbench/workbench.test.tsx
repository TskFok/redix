import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import App from "../../App";
import type { CommandResult, ConnectionProfile } from "../../lib/types";
import WorkbenchPage from "./WorkbenchPage";

const {
  executeCommandMock,
  listConnectionsMock,
  openConnectionMock,
  saveConnectionMock,
  deleteConnectionMock,
  testConnectionMock,
  scanKeysMock,
  getKeyMock,
  setKeyMock,
  deleteKeyMock,
  setKeyTtlMock,
} = vi.hoisted(() => ({
  executeCommandMock: vi.fn(),
  listConnectionsMock: vi.fn(),
  openConnectionMock: vi.fn(),
  saveConnectionMock: vi.fn(),
  deleteConnectionMock: vi.fn(),
  testConnectionMock: vi.fn(),
  scanKeysMock: vi.fn(),
  getKeyMock: vi.fn(),
  setKeyMock: vi.fn(),
  deleteKeyMock: vi.fn(),
  setKeyTtlMock: vi.fn(),
}));

vi.mock("../../lib/tauri", () => ({
  executeCommand: executeCommandMock,
  listConnections: listConnectionsMock,
  openConnection: openConnectionMock,
  saveConnection: saveConnectionMock,
  deleteConnection: deleteConnectionMock,
  testConnection: testConnectionMock,
  scanKeys: scanKeysMock,
  getKey: getKeyMock,
  setKey: setKeyMock,
  deleteKey: deleteKeyMock,
  setKeyTtl: setKeyTtlMock,
}));

const localProfile: ConnectionProfile = {
  id: "local",
  name: "本地 Redis",
  host: "127.0.0.1",
  port: 6379,
  username: null,
  database: 0,
  has_password: false,
};

function typeCommand(command: string) {
  fireEvent.change(screen.getByRole("textbox", { name: "Redis 命令" }), {
    target: { value: command },
  });
}

describe("Redis Workbench 工作区", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    listConnectionsMock.mockResolvedValue([]);
    openConnectionMock.mockResolvedValue({ server_version: "8.4.0" });
    saveConnectionMock.mockResolvedValue(localProfile);
    deleteConnectionMock.mockResolvedValue(undefined);
    testConnectionMock.mockResolvedValue({ server_version: "8.4.0" });
    scanKeysMock.mockResolvedValue({ cursor: 0, keys: [], has_more: false });
    getKeyMock.mockRejectedValue({ code: "KEY_NOT_FOUND", message: "键不存在" });
    setKeyMock.mockResolvedValue(undefined);
    deleteKeyMock.mockResolvedValue(undefined);
    setKeyTtlMock.mockResolvedValue(-1);
    vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => undefined);
  });

  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
  });

  it("没有连接时禁用执行并显示明确提示", () => {
    render(<WorkbenchPage connectionId={null} />);

    expect(screen.getByRole("button", { name: "执行" })).toBeDisabled();
    expect(screen.getByRole("status")).toHaveTextContent("请先连接 Redis");
  });

  it("命令为空时禁用执行", () => {
    render(<WorkbenchPage connectionId="local" />);

    expect(screen.getByRole("button", { name: "执行" })).toBeDisabled();
  });

  it("执行命令并展示字符串结果", async () => {
    executeCommandMock.mockResolvedValue({ kind: "string", value: "PONG" });
    render(<WorkbenchPage connectionId="local" />);

    typeCommand("PING");
    fireEvent.click(screen.getByRole("button", { name: "执行" }));

    expect(executeCommandMock).toHaveBeenCalledWith({
      connection_id: "local",
      command: "PING",
    });
    expect(await screen.findByText("PONG")).toBeInTheDocument();
    expect(Storage.prototype.setItem).not.toHaveBeenCalled();
  });

  it("使用 Cmd 或 Ctrl 加 Enter 执行当前命令", async () => {
    executeCommandMock.mockResolvedValue({ kind: "number", value: 3 });
    render(<WorkbenchPage connectionId="local" />);

    typeCommand("DBSIZE");
    fireEvent.keyDown(screen.getByRole("textbox", { name: "Redis 命令" }), {
      key: "Enter",
      code: "Enter",
      ctrlKey: true,
    });

    await waitFor(() => expect(executeCommandMock).toHaveBeenCalledTimes(1));
    expect(executeCommandMock).toHaveBeenCalledWith({
      connection_id: "local",
      command: "DBSIZE",
    });
  });

  it("执行期间显示 loading 并阻止重复提交", async () => {
    let resolveCommand: ((result: CommandResult) => void) | undefined;
    executeCommandMock.mockImplementation(
      () => new Promise<CommandResult>((resolve) => {
        resolveCommand = resolve;
      }),
    );
    render(<WorkbenchPage connectionId="local" />);

    typeCommand("PING");
    const executeButton = screen.getByRole("button", { name: "执行" });
    fireEvent.click(executeButton);
    expect(executeButton).toBeDisabled();
    expect(screen.getByRole("status")).toHaveTextContent("执行中");

    fireEvent.click(executeButton);
    expect(executeCommandMock).toHaveBeenCalledTimes(1);

    resolveCommand?.({ kind: "string", value: "PONG" });
    expect(await screen.findByText("PONG")).toBeInTheDocument();
  });

  it("渲染标量、布尔值、空值和嵌套数组结果且保留完整内容", async () => {
    const largeValue = "x".repeat(4096);
    executeCommandMock.mockResolvedValue({
      kind: "array",
      value: ["item", 42, true, null, { nested: ["value", largeValue] }],
    });
    render(<WorkbenchPage connectionId="local" />);

    typeCommand("DEBUG RESULT");
    fireEvent.click(screen.getByRole("button", { name: "执行" }));

    const resultGroup = await screen.findByRole("group", { name: "命令结果" });
    expect(resultGroup).toHaveTextContent("item");
    expect(resultGroup).toHaveTextContent("42");
    expect(resultGroup).toHaveTextContent("true");
    expect(resultGroup).toHaveTextContent("null");
    expect(resultGroup).toHaveTextContent(largeValue);
  });

  it("失败时只显示稳定错误码和消息，不泄露 URI 或密码", async () => {
    executeCommandMock.mockRejectedValue({
      code: "COMMAND_FAILED",
      message: "执行命令失败",
      uri: "redis://:secret-value@127.0.0.1:6379/0",
      password: "secret-value",
      cause: "raw redis error",
    });
    render(<WorkbenchPage connectionId="local" />);

    typeCommand("PING");
    fireEvent.click(screen.getByRole("button", { name: "执行" }));

    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("COMMAND_FAILED");
    expect(alert).toHaveTextContent("执行命令失败");
    expect(alert).not.toHaveTextContent("redis://");
    expect(alert).not.toHaveTextContent("secret-value");
    expect(alert).not.toHaveTextContent("raw redis error");
  });

  it("历史记录按最近在前，点击只回填命令而不自动执行", async () => {
    executeCommandMock
      .mockResolvedValueOnce({ kind: "string", value: "PONG" })
      .mockResolvedValueOnce({ kind: "number", value: 2 });
    render(<WorkbenchPage connectionId="local" />);

    typeCommand("PING");
    fireEvent.click(screen.getByRole("button", { name: "执行" }));
    await screen.findByText("PONG");

    typeCommand("DBSIZE");
    fireEvent.click(screen.getByRole("button", { name: "执行" }));
    await screen.findByText("2");

    const history = screen.getByRole("list", { name: "命令历史" });
    const historyItems = history.querySelectorAll("button");
    expect(historyItems[0]).toHaveTextContent("DBSIZE");
    expect(historyItems[1]).toHaveTextContent("PING");

    fireEvent.click(historyItems[1]);
    expect(screen.getByRole("textbox", { name: "Redis 命令" })).toHaveValue("PING");
    expect(executeCommandMock).toHaveBeenCalledTimes(2);
  });

  it("没有活动连接时导航明确禁用，有活动连接时可切换 Workbench", async () => {
    const { unmount } = render(<App />);

    const disabledWorkbench = await screen.findByRole("button", { name: "Workbench" });
    expect(disabledWorkbench).toBeDisabled();
    expect(screen.getByText("请先连接 Redis 后使用工作区。" )).toBeInTheDocument();

    unmount();
    listConnectionsMock.mockResolvedValue([localProfile]);
    render(<App />);
    await screen.findByRole("button", { name: "连接" });
    fireEvent.click(screen.getByRole("button", { name: "连接" }));
    await waitFor(() => expect(openConnectionMock).toHaveBeenCalledWith("local"));

    fireEvent.click(screen.getByRole("button", { name: "Workbench" }));
    expect(screen.getByRole("heading", { name: "Workbench" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Workbench" })).toHaveAttribute(
      "aria-current",
      "page",
    );
  });
});
