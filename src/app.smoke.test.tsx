import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import App from "./App";

const {
  listConnectionsMock,
  openConnectionMock,
  closeConnectionMock,
  scanKeysMock,
  executeCommandMock,
  executeCommandsMock,
  getCommandCatalogMock,
  getDatabaseOverviewMock,
  getInstanceOverviewMock,
  listCommandHistoryMock,
  saveCommandHistoryMock,
  selectDatabaseMock,
} = vi.hoisted(() => ({
  listConnectionsMock: vi.fn(),
  openConnectionMock: vi.fn(),
  closeConnectionMock: vi.fn(),
  scanKeysMock: vi.fn(),
  executeCommandMock: vi.fn(),
  executeCommandsMock: vi.fn(),
  getCommandCatalogMock: vi.fn(),
  getDatabaseOverviewMock: vi.fn(),
  getInstanceOverviewMock: vi.fn(),
  listCommandHistoryMock: vi.fn(),
  saveCommandHistoryMock: vi.fn(),
  selectDatabaseMock: vi.fn(),
}));

vi.mock("./lib/tauri", () => ({
  listConnections: listConnectionsMock,
  openConnection: openConnectionMock,
  closeConnection: closeConnectionMock,
  scanKeys: scanKeysMock,
  executeCommand: executeCommandMock,
  executeCommands: executeCommandsMock,
  getCommandCatalog: getCommandCatalogMock,
  getDatabaseOverview: getDatabaseOverviewMock,
  getInstanceOverview: getInstanceOverviewMock,
  listCommandHistory: listCommandHistoryMock,
  saveCommandHistory: saveCommandHistoryMock,
  selectDatabase: selectDatabaseMock,
  deleteConnection: vi.fn(),
  saveConnection: vi.fn(),
  testConnection: vi.fn(),
  getKey: vi.fn(),
  setKey: vi.fn(),
  createKey: vi.fn(),
  renameKey: vi.fn(),
  deleteKey: vi.fn(),
  deleteKeys: vi.fn(),
  setKeyTtl: vi.fn(),
  getKeyInfo: vi.fn(),
  exportKeys: vi.fn(),
  importKeys: vi.fn(),
}));

const localProfile = {
  id: "local",
  name: "本地 Redis",
  host: "127.0.0.1",
  port: 6379,
  username: null,
  database: 0,
  has_password: false,
};

beforeEach(() => {
  vi.clearAllMocks();
  listConnectionsMock.mockResolvedValue([]);
  openConnectionMock.mockResolvedValue({ server_version: "8.4.0" });
  closeConnectionMock.mockResolvedValue(undefined);
  scanKeysMock.mockResolvedValue({ cursor: 0, keys: [], has_more: false });
  executeCommandMock.mockResolvedValue({ kind: "string", value: "PONG" });
  executeCommandsMock.mockResolvedValue([]);
  getCommandCatalogMock.mockResolvedValue([
    { name: "PING", summary: "检查 Redis 连接", arguments: [] },
  ]);
  listCommandHistoryMock.mockResolvedValue([]);
  saveCommandHistoryMock.mockResolvedValue(undefined);
  getInstanceOverviewMock.mockResolvedValue({
    server_version: "8.4.0",
    redis_mode: "standalone",
    uptime_seconds: 42,
    connected_clients: 1,
    used_memory_bytes: 1024,
    max_memory_bytes: null,
    total_commands_processed: 1,
    keyspace_hits: 1,
    keyspace_misses: 0,
    role: "master",
    modules: [],
  });
  getDatabaseOverviewMock.mockResolvedValue([
    { database: 0, key_count: 1, expires: 0, avg_ttl_ms: 0 },
  ]);
  selectDatabaseMock.mockResolvedValue(localProfile);
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

describe("Redix 应用壳", () => {
  it("显示应用名称和默认工作区", () => {
    render(<App />);

    expect(screen.getByRole("heading", { name: "Redix" })).toBeInTheDocument();
    expect(screen.getByText("Browser")).toBeInTheDocument();
    expect(screen.getByText("Workbench")).toBeInTheDocument();
  });

  it("未连接时以连接管理为默认入口并禁用数据工作区", () => {
    render(<App />);

    const navigation = screen.getByRole("navigation", { name: "主导航" });
    expect(navigation).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "连接管理" })).toHaveAttribute(
      "aria-current",
      "page",
    );
    expect(screen.getByRole("button", { name: "Browser" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Workbench" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Database" })).toBeDisabled();
  });

  it("连接后显示 Database 工作区并可加载概览", async () => {
    listConnectionsMock.mockResolvedValue([localProfile]);
    render(<App />);

    fireEvent.click(await screen.findByRole("button", { name: "连接" }));
    await waitFor(() => expect(openConnectionMock).toHaveBeenCalledWith("local"));
    const databaseButton = screen.getByRole("button", { name: "Database" });
    expect(databaseButton).toBeEnabled();
    fireEvent.click(databaseButton);

    expect(await screen.findByRole("heading", { name: "数据库概览" })).toBeInTheDocument();
  });

  it("已连接时 Workbench 显示命令目录、批量策略和结果格式控件", async () => {
    listConnectionsMock.mockResolvedValue([localProfile]);
    render(<App />);

    fireEvent.click(await screen.findByRole("button", { name: "连接" }));
    await waitFor(() => expect(openConnectionMock).toHaveBeenCalledWith("local"));
    fireEvent.click(screen.getByRole("button", { name: "Workbench" }));

    expect(await screen.findByText("支持多行批量执行")).toBeInTheDocument();
    expect(screen.getByText("批量命令遇错后继续")).toBeInTheDocument();
    fireEvent.change(screen.getByRole("textbox", { name: "Redis 命令" }), {
      target: { value: "PING" },
    });
    fireEvent.click(screen.getByRole("button", { name: "执行" }));
    expect(await screen.findByText("PONG")).toBeInTheDocument();
    expect(screen.getByLabelText("结果格式")).toBeInTheDocument();
  });
});
