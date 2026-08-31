import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import App from "../../App";
import type { CommandResult, ConnectionProfile } from "../../lib/types";
import WorkbenchPage from "./WorkbenchPage";

const {
  executeCommandsMock,
  executeCommandMock,
  getCommandCatalogMock,
  getAppSettingsMock,
  listCommandHistoryMock,
  saveCommandHistoryMock,
  listConnectionsMock,
  openConnectionMock,
  closeConnectionMock,
  saveConnectionMock,
  deleteConnectionMock,
  testConnectionMock,
  scanKeysMock,
  getModuleCapabilitiesMock,
  getKeyMock,
  setKeyMock,
  deleteKeyMock,
  setKeyTtlMock,
} = vi.hoisted(() => ({
  executeCommandsMock: vi.fn(),
  executeCommandMock: vi.fn(),
  getCommandCatalogMock: vi.fn(),
  getAppSettingsMock: vi.fn(),
  listCommandHistoryMock: vi.fn(),
  saveCommandHistoryMock: vi.fn(),
  listConnectionsMock: vi.fn(),
  openConnectionMock: vi.fn(),
  closeConnectionMock: vi.fn(),
  saveConnectionMock: vi.fn(),
  deleteConnectionMock: vi.fn(),
  testConnectionMock: vi.fn(),
  scanKeysMock: vi.fn(),
  getModuleCapabilitiesMock: vi.fn(),
  getKeyMock: vi.fn(),
  setKeyMock: vi.fn(),
  deleteKeyMock: vi.fn(),
  setKeyTtlMock: vi.fn(),
}));

vi.mock("../../lib/tauri", () => ({
  executeCommands: executeCommandsMock,
  executeCommand: executeCommandMock,
  getCommandCatalog: getCommandCatalogMock,
  getAppSettings: getAppSettingsMock,
  listCommandHistory: listCommandHistoryMock,
  saveCommandHistory: saveCommandHistoryMock,
  listConnections: listConnectionsMock,
  openConnection: openConnectionMock,
  closeConnection: closeConnectionMock,
  saveConnection: saveConnectionMock,
  deleteConnection: deleteConnectionMock,
  testConnection: testConnectionMock,
  scanKeys: scanKeysMock,
  getModuleCapabilities: getModuleCapabilitiesMock,
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
  tls: false,
  verify_server_cert: true,
  ca_certificate_name: null,
  client_certificate_name: null,
  has_ca_certificate: false,
  has_client_certificate: false,
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
    closeConnectionMock.mockResolvedValue(undefined);
    saveConnectionMock.mockResolvedValue(localProfile);
    deleteConnectionMock.mockResolvedValue(undefined);
    testConnectionMock.mockResolvedValue({ server_version: "8.4.0" });
    scanKeysMock.mockResolvedValue({ cursor: 0, keys: [], has_more: false });
    getModuleCapabilitiesMock.mockResolvedValue({
      modules: [],
      json_supported: false,
      json_version: null,
      search_supported: false,
      search_version: null,
      array_supported: false,
      vector_set_supported: false,
    });
    getKeyMock.mockRejectedValue({ code: "KEY_NOT_FOUND", message: "键不存在" });
    setKeyMock.mockResolvedValue(undefined);
    deleteKeyMock.mockResolvedValue(undefined);
    setKeyTtlMock.mockResolvedValue(-1);
    getCommandCatalogMock.mockResolvedValue([
      { name: "PING", summary: "检查 Redis 连接", arguments: [] },
      { name: "GET", summary: "读取字符串键", arguments: [{ name: "key", required: true, hint: "键名" }] },
      { name: "SET", summary: "写入字符串键", arguments: [{ name: "key", required: true, hint: "键名" }] },
    ]);
    getAppSettingsMock.mockResolvedValue({
      version: 1,
      theme: "system",
      result_format: "text",
      scan_count: 100,
      continue_on_error: false,
    });
    listCommandHistoryMock.mockResolvedValue([]);
    saveCommandHistoryMock.mockResolvedValue(undefined);
    vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => undefined);
  });

  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
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

  it("仅空白命令时禁用执行且不调用 IPC", () => {
    render(<WorkbenchPage connectionId="local" />);

    typeCommand("  \t  ");
    expect(screen.getByRole("button", { name: "执行" })).toBeDisabled();
    fireEvent.click(screen.getByRole("button", { name: "执行" }));
    expect(executeCommandMock).not.toHaveBeenCalled();
  });

  it("空字符串连接 ID 时禁用执行且不调用 IPC", () => {
    render(<WorkbenchPage connectionId="" />);

    expect(screen.getByRole("button", { name: "执行" })).toBeDisabled();
    fireEvent.change(screen.getByRole("textbox", { name: "Redis 命令" }), {
      target: { value: "PING" },
    });
    expect(screen.getByRole("button", { name: "执行" })).toBeDisabled();
    fireEvent.click(screen.getByRole("button", { name: "执行" }));
    expect(executeCommandMock).not.toHaveBeenCalled();
  });

  it("仅空白连接 ID 时禁用执行且不调用 IPC", () => {
    render(<WorkbenchPage connectionId="   " />);

    expect(screen.getByRole("button", { name: "执行" })).toBeDisabled();
    expect(screen.getByText("未连接")).toBeInTheDocument();
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

  it("一次 IPC 执行多条命令并按顺序展示结果", async () => {
    executeCommandsMock.mockResolvedValue([
      { command: "PING", result: { kind: "string", value: "PONG" }, error_code: null },
      { command: "DBSIZE", result: { kind: "number", value: 2 }, error_code: null },
    ]);
    render(<WorkbenchPage connectionId="local" />);
    typeCommand("PING\nDBSIZE");
    fireEvent.click(screen.getByRole("button", { name: "执行" }));

    expect(await screen.findByText("PONG")).toBeInTheDocument();
    expect(screen.getByText("2")).toBeInTheDocument();
    expect(executeCommandsMock).toHaveBeenCalledTimes(1);
    expect(executeCommandsMock).toHaveBeenCalledWith({
      connection_id: "local",
      commands: ["PING", "DBSIZE"],
      continue_on_error: false,
    });
  });

  it("可切换结果格式并复制当前返回值", async () => {
    executeCommandMock.mockResolvedValue({
      kind: "array",
      value: { key: "value", nested: [1, 2] },
    });
    const writeText = vi.fn().mockResolvedValue(undefined);
    vi.stubGlobal("navigator", { clipboard: { writeText } });
    render(<WorkbenchPage connectionId="local" />);
    typeCommand("INFO");
    fireEvent.click(screen.getByRole("button", { name: "执行" }));
    await screen.findByText(/key/);

    fireEvent.change(screen.getByLabelText("结果格式"), { target: { value: "json" } });
    expect(screen.getByText(/"nested"/)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "复制结果" }));
    await waitFor(() => expect(writeText).toHaveBeenCalled());
    expect(writeText.mock.calls[0][0]).toContain('"key"');
  });

  it("应用设置提供 Workbench 的初始结果格式和批量错误策略", async () => {
    executeCommandsMock.mockResolvedValue([
      { command: "PING", result: { kind: "string", value: "PONG" }, error_code: null },
      { command: "DBSIZE", result: { kind: "number", value: 2 }, error_code: null },
    ]);
    render(
      <WorkbenchPage
        connectionId="local"
        defaultFormat="json"
        defaultContinueOnError
      />,
    );

    expect(screen.getByLabelText("批量命令遇错后继续")).toBeChecked();
    typeCommand("PING\nDBSIZE");
    fireEvent.click(screen.getByRole("button", { name: "执行" }));

    await screen.findByText('"PONG"');
    expect(executeCommandsMock).toHaveBeenCalledWith({
      connection_id: "local",
      commands: ["PING", "DBSIZE"],
      continue_on_error: true,
    });
  });

  it("根据本地命令目录显示提示并回填而不自动执行", async () => {
    render(<WorkbenchPage connectionId="local" />);
    typeCommand("PI");
    expect(await screen.findByRole("option", { name: /PING/ })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("option", { name: /PING/ }));
    expect(screen.getByRole("textbox", { name: "Redis 命令" })).toHaveValue("PING");
    expect(executeCommandMock).not.toHaveBeenCalled();
  });

  it("加载并保存当前连接历史，敏感命令不进入保存批次", async () => {
    listCommandHistoryMock.mockResolvedValue([
      {
        connection_id: "local",
        command: "DBSIZE",
        result: { kind: "number", value: 2 },
        error_code: null,
        created_at: "2026-08-19T00:00:00Z",
      },
    ]);
    executeCommandMock.mockResolvedValue({ kind: "string", value: "PONG" });
    render(<WorkbenchPage connectionId="local" />);
    expect(await screen.findByRole("button", { name: /回填命令 DBSIZE/ })).toBeInTheDocument();

    typeCommand("AUTH secret");
    fireEvent.click(screen.getByRole("button", { name: "执行" }));
    await screen.findByText("PONG");
    expect(saveCommandHistoryMock).toHaveBeenCalledTimes(1);
    const savedEntries = saveCommandHistoryMock.mock.calls[0][0].entries;
    expect(savedEntries).toEqual(
      expect.arrayContaining([expect.objectContaining({ command: "DBSIZE" })]),
    );
    expect(savedEntries).not.toEqual(
      expect.arrayContaining([expect.objectContaining({ command: "AUTH secret" })]),
    );
  });

  it("执行前只去除命令首尾空白并保留引号和内部空白", async () => {
    executeCommandMock.mockResolvedValue({ kind: "string", value: "OK" });
    render(<WorkbenchPage connectionId=" local " />);

    typeCommand('  SET "key with space"  value  ');
    fireEvent.click(screen.getByRole("button", { name: "执行" }));

    expect(executeCommandMock).toHaveBeenCalledWith({
      connection_id: "local",
      command: 'SET "key with space"  value',
    });
    await screen.findByText("OK");
    expect(
      screen.getByRole("button", { name: /回填命令 SET/ }).querySelector("code")?.textContent,
    ).toBe('SET "key with space"  value');
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
      message: "redis://:secret-value@127.0.0.1:6379/0 底层错误",
      uri: "redis://:secret-value@127.0.0.1:6379/0",
      password: "secret-value",
      cause: "raw redis error",
    });
    render(<WorkbenchPage connectionId="local" />);

    typeCommand("PING");
    fireEvent.click(screen.getByRole("button", { name: "执行" }));

    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("COMMAND_FAILED");
    expect(alert).toHaveTextContent("Redis 操作失败，请稍后重试。");
    expect(alert).not.toHaveTextContent("redis://");
    expect(alert).not.toHaveTextContent("secret-value");
    expect(alert).not.toHaveTextContent("raw redis error");
  });

  it("未知错误码和敏感 message 都降级为稳定安全错误契约", async () => {
    executeCommandMock.mockRejectedValue({
      code: "UNTRUSTED_ERROR",
      message: "redis://:another-secret@127.0.0.1:6379/0 raw redis failure",
    });
    render(<WorkbenchPage connectionId="local" />);

    typeCommand("PING");
    fireEvent.click(screen.getByRole("button", { name: "执行" }));

    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("COMMAND_FAILED");
    expect(alert).toHaveTextContent("Redis 操作失败，请稍后重试。");
    expect(alert).not.toHaveTextContent("UNTRUSTED_ERROR");
    expect(alert).not.toHaveTextContent("another-secret");
    expect(alert).not.toHaveTextContent("raw redis failure");
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
    const historyItems = history.querySelectorAll('button[aria-label^="回填命令"]');
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
