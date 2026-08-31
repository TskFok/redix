import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import WorkbenchPage from "./WorkbenchPage";
import { currentCommandLine, filterCommandCatalog } from "./CommandSuggestions";
import { normalizeCommandList, isCommandReady } from "./workbenchState";
import { filterHistoryEntry } from "./workbenchHistory";

const ipc = vi.hoisted(() => ({
  executeCommand: vi.fn(), executeCommands: vi.fn(), getCommandCatalog: vi.fn(),
  listCommandHistory: vi.fn(), saveCommandHistory: vi.fn(),
  deleteCommandHistory: vi.fn(), clearCommandHistory: vi.fn(),
}));
vi.mock("../../lib/tauri", () => ipc);
const catalog = [{ name: "JSON.GET", summary: "读取 JSON", arguments: [
  { name: "key", required: true, hint: "键名" },
  { name: "path", required: false, hint: "JSONPath" },
] }];
const entry = { connection_id: "local", command: "PING", result: null, error_code: null, created_at: "2026-08-31T00:00:00Z" };

beforeEach(() => {
  vi.resetAllMocks();
  ipc.getCommandCatalog.mockResolvedValue(catalog);
  ipc.listCommandHistory.mockResolvedValue([]);
  ipc.saveCommandHistory.mockResolvedValue(undefined);
  ipc.deleteCommandHistory.mockResolvedValue(undefined);
  ipc.clearCommandHistory.mockResolvedValue(undefined);
});
afterEach(cleanup);

describe("Workbench 高级交互", () => {
  it("跳过脚本整行注释但保留字符串中的井号", () => {
    expect(normalizeCommandList('# 注释\n  // comment\nSET a "#value"\nPING')).toEqual(['SET a "#value"', 'PING']);
    expect(isCommandReady("local", "# only comment")).toBe(false);
    expect(filterCommandCatalog("# JSON", catalog)).toEqual([]);
  });

  it("光标在开头空行时不会读取第二行命令", () => {
    expect(currentCommandLine("\nJSON.GET key", 0)).toEqual({ start: 0, end: 0, text: "" });
  });

  it("敏感命令名称使用引号或转义时也不存入历史", () => {
    expect(filterHistoryEntry('"AUTH" secret')).toBe(false);
    expect(filterHistoryEntry("A\\UTH secret")).toBe(false);
    expect(filterHistoryEntry("GET secret")).toBe(true);
    expect(filterHistoryEntry("'AUTH secret")).toBe(false);
  });

  it("光标位于首行时仅补全该行并保留缩进和后续命令", async () => {
    render(<WorkbenchPage connectionId="local" />);
    await waitFor(() => expect(ipc.getCommandCatalog).toHaveBeenCalled());
    const input = screen.getByRole("textbox", { name: "Redis 命令" }) as HTMLTextAreaElement;
    fireEvent.change(input, { target: { value: "  JSON.G\nPING", selectionStart: 8 } });
    input.setSelectionRange(8, 8);
    fireEvent.select(input);
    const option = await screen.findByRole("option", { name: /JSON.GET/ });
    fireEvent.click(option);
    expect(input).toHaveValue("  JSON.GET\nPING");
    expect(input.selectionStart).toBe(10);
  });

  it("光标进入参数后显示本地参数帮助且注释中不显示", async () => {
    render(<WorkbenchPage connectionId="local" />);
    const input = screen.getByRole("textbox", { name: "Redis 命令" });
    fireEvent.change(input, { target: { value: "JSON.GET document $.profile" } });
    const help = await screen.findByRole("region", { name: "当前命令参数" });
    expect(help).toHaveTextContent("JSONPath");
    expect(help).toHaveTextContent("必填");
    fireEvent.change(input, { target: { value: "# JSON.GET" } });
    expect(screen.queryByRole("region", { name: "当前命令参数" })).not.toBeInTheDocument();
  });

  it("嵌套结果提供树和表格，同时保留 JSON", async () => {
    ipc.executeCommand.mockResolvedValue({ kind: "array", value: [["1700-0", ["name", "Ada"]], null] });
    render(<WorkbenchPage connectionId="local" />);
    fireEvent.change(screen.getByRole("textbox", { name: "Redis 命令" }), { target: { value: "XRANGE stream - +" } });
    fireEvent.click(screen.getByRole("button", { name: "执行" }));
    const select = await screen.findByRole("combobox", { name: "结果格式" });
    fireEvent.change(select, { target: { value: "tree" } });
    const tree = screen.getByRole("tree", { name: "RESP 结果树" });
    const root = within(tree).getAllByRole("button")[0];
    expect(root).toHaveAttribute("aria-expanded", "true");
    fireEvent.click(root);
    expect(root).toHaveAttribute("aria-expanded", "false");
    fireEvent.click(root);
    expect(tree).toHaveTextContent("null");
    fireEvent.change(select, { target: { value: "table" } });
    expect(screen.getByRole("table", { name: "RESP 结果表格" })).toHaveTextContent("1700-0");
    fireEvent.change(select, { target: { value: "json" } });
    expect(screen.getByRole("group", { name: "命令结果" })).toHaveTextContent('"Ada"');
  });

  it("单条删除失败保留记录，重试后可清空当前连接历史", async () => {
    ipc.listCommandHistory.mockResolvedValue([entry, { ...entry, command: "DBSIZE" }]);
    ipc.deleteCommandHistory.mockRejectedValueOnce({ code: "PERSISTENCE_FAILED" }).mockResolvedValue(undefined);
    render(<WorkbenchPage connectionId="local" />);
    fireEvent.click(await screen.findByRole("button", { name: "删除历史 PING" }));
    await screen.findByRole("alert");
    expect(screen.getByRole("button", { name: "回填命令 PING" })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "删除历史 PING" }));
    await waitFor(() => expect(screen.queryByRole("button", { name: "回填命令 PING" })).not.toBeInTheDocument());
    expect(ipc.deleteCommandHistory).toHaveBeenLastCalledWith({ connection_id: "local", command: "PING", created_at: entry.created_at });
    fireEvent.click(screen.getByRole("button", { name: "清空历史" }));
    await waitFor(() => expect(screen.queryByRole("list", { name: "命令历史" })).not.toBeInTheDocument());
    expect(ipc.clearCommandHistory).toHaveBeenCalledWith({ connection_id: "local" });
  });

  it("删除等待未完成的历史保存，避免旧保存恢复已删除条目", async () => {
    let finishSave: (() => void) | undefined;
    ipc.saveCommandHistory.mockImplementation(() => new Promise<void>((resolve) => { finishSave = resolve; }));
    ipc.executeCommand.mockResolvedValue({ kind: "string", value: "PONG" });
    render(<WorkbenchPage connectionId="local" />);
    fireEvent.change(screen.getByRole("textbox", { name: "Redis 命令" }), { target: { value: "PING" } });
    fireEvent.click(screen.getByRole("button", { name: "执行" }));
    fireEvent.click(await screen.findByRole("button", { name: "删除历史 PING" }));
    expect(ipc.deleteCommandHistory).not.toHaveBeenCalled();
    expect(screen.getByRole("button", { name: "执行" })).toBeDisabled();
    finishSave?.();
    await waitFor(() => expect(screen.queryByRole("button", { name: "回填命令 PING" })).not.toBeInTheDocument());
  });

  it("连接切换后忽略旧历史清空结果", async () => {
    let finishClear: (() => void) | undefined;
    ipc.listCommandHistory.mockImplementation((id) => Promise.resolve([{ ...entry, connection_id: id, command: id === "local" ? "PING" : "DBSIZE" }]));
    ipc.clearCommandHistory.mockImplementation(() => new Promise<void>((resolve) => { finishClear = resolve; }));
    const view = render(<WorkbenchPage connectionId="local" />);
    await screen.findByRole("button", { name: "回填命令 PING" });
    fireEvent.click(screen.getByRole("button", { name: "清空历史" }));
    await waitFor(() => expect(ipc.clearCommandHistory).toHaveBeenCalled());
    view.rerender(<WorkbenchPage connectionId="other" />);
    await screen.findByRole("button", { name: "回填命令 DBSIZE" });
    finishClear?.();
    await waitFor(() => expect(screen.getByRole("button", { name: "回填命令 DBSIZE" })).toBeInTheDocument());
  });
});
