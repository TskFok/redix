import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import App from "./App";
import type { KeySummary } from "./lib/types";

vi.mock("./features/tasks/bulkTaskApi", async (importOriginal) => ({
  ...await importOriginal<typeof import("./features/tasks/bulkTaskApi")>(),
  listBulkTasks: vi.fn().mockResolvedValue([]),
}));

const {
  listConnectionsMock,
  openConnectionMock,
  closeConnectionMock,
  deleteConnectionMock,
  scanAllKeysMock,
  getModuleCapabilitiesMock,
  listSearchIndexesMock,
  getSearchIndexMock,
  searchKeysMock,
  executeCommandMock,
  executeCommandsMock,
  getCommandCatalogMock,
  getDatabaseOverviewMock,
  getInstanceDetailsMock,
  getInstanceOverviewMock,
  analyzeDatabaseMock,
  getAppSettingsMock,
  listQueryLibraryMock,
  listCommandHistoryMock,
  saveAppSettingsMock,
  saveQueryLibraryItemMock,
  deleteQueryLibraryItemMock,
  saveCommandHistoryMock,
  selectDatabaseMock,
} = vi.hoisted(() => ({
  listConnectionsMock: vi.fn(),
  openConnectionMock: vi.fn(),
  closeConnectionMock: vi.fn(),
  deleteConnectionMock: vi.fn(),
  scanAllKeysMock: vi.fn(),
  getModuleCapabilitiesMock: vi.fn(),
  listSearchIndexesMock: vi.fn(),
  getSearchIndexMock: vi.fn(),
  searchKeysMock: vi.fn(),
  executeCommandMock: vi.fn(),
  executeCommandsMock: vi.fn(),
  getCommandCatalogMock: vi.fn(),
  getDatabaseOverviewMock: vi.fn(),
  getInstanceDetailsMock: vi.fn(),
  getInstanceOverviewMock: vi.fn(),
  analyzeDatabaseMock: vi.fn(),
  getAppSettingsMock: vi.fn(),
  listQueryLibraryMock: vi.fn(),
  listCommandHistoryMock: vi.fn(),
  saveAppSettingsMock: vi.fn(),
  saveQueryLibraryItemMock: vi.fn(),
  deleteQueryLibraryItemMock: vi.fn(),
  saveCommandHistoryMock: vi.fn(),
  selectDatabaseMock: vi.fn(),
}));

vi.mock("./lib/tauri", () => ({
  listConnections: listConnectionsMock,
  openConnection: openConnectionMock,
  closeConnection: closeConnectionMock,
  scanAllKeys: scanAllKeysMock,
  getModuleCapabilities: getModuleCapabilitiesMock,
  listSearchIndexes: listSearchIndexesMock,
  getSearchIndex: getSearchIndexMock,
  searchKeys: searchKeysMock,
  executeCommand: executeCommandMock,
  executeCommands: executeCommandsMock,
  getCommandCatalog: getCommandCatalogMock,
  getDatabaseOverview: getDatabaseOverviewMock,
  getInstanceDetails: getInstanceDetailsMock,
  getInstanceOverview: getInstanceOverviewMock,
  analyzeDatabase: analyzeDatabaseMock,
  getAppSettings: getAppSettingsMock,
  listQueryLibrary: listQueryLibraryMock,
  listCommandHistory: listCommandHistoryMock,
  saveAppSettings: saveAppSettingsMock,
  saveQueryLibraryItem: saveQueryLibraryItemMock,
  deleteQueryLibraryItem: deleteQueryLibraryItemMock,
  saveCommandHistory: saveCommandHistoryMock,
  selectDatabase: selectDatabaseMock,
  getClusterTopology: vi.fn().mockResolvedValue({ summary: { state: "ok", slots_assigned: 16384, slots_ok: 16384, slots_pfail: 0, slots_fail: 0, current_epoch: 1, size: 0, known_nodes: 0 }, nodes: [], failures: [] }),
  refreshClusterTopology: vi.fn(),
  deleteConnection: deleteConnectionMock,
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
  tls: false,
  verify_server_cert: true,
  ca_certificate_name: null,
  client_certificate_name: null,
  has_ca_certificate: false,
  has_client_certificate: false,
};

it("只为 Cluster 提供拓扑导航且全局上下文展示全部种子", async () => {
  listConnectionsMock.mockResolvedValue([{ ...localProfile, cluster: { nodes: [{ host: "redis-a", port: 7000 }, { host: "redis-b", port: 7001 }], read_from_replicas: false } }]);
  render(<App />);
  expect(screen.queryByRole("button", { name: "Cluster 拓扑" })).not.toBeInTheDocument();
  fireEvent.click(await screen.findByRole("button", { name: "连接" }));
  fireEvent.click(await screen.findByRole("button", { name: "Cluster 拓扑" }));
  expect(await screen.findByText("暂无拓扑节点")).toBeInTheDocument();
  expect(screen.getByText(/当前连接：/)).toHaveTextContent("redis-a:7000、redis-b:7001");
});

beforeEach(() => {
  vi.clearAllMocks();
  listConnectionsMock.mockResolvedValue([]);
  openConnectionMock.mockResolvedValue({ server_version: "8.4.0" });
  closeConnectionMock.mockResolvedValue(undefined);
  deleteConnectionMock.mockResolvedValue(undefined);
  scanAllKeysMock.mockResolvedValue([]);
  getModuleCapabilitiesMock.mockResolvedValue({
    modules: [],
    json_supported: false,
    json_version: null,
    search_supported: false,
    search_version: null,
    array_supported: false,
    vector_set_supported: false,
  });
  listSearchIndexesMock.mockResolvedValue({ indexes: [] });
  getSearchIndexMock.mockResolvedValue(null);
  searchKeysMock.mockResolvedValue({
    total: 0,
    offset: 0,
    next_offset: null,
    max_results: 100,
    keys: [],
  });
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
  getInstanceDetailsMock.mockResolvedValue({
    overview: {
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
    },
    clients: { connected_clients: 1, blocked_clients: 0, tracking_clients: 0, max_clients: 10000 },
    memory: {
      used_memory_bytes: 1024,
      used_memory_peak_bytes: 1024,
      used_memory_rss_bytes: 1024,
      mem_fragmentation_ratio: 1,
      allocator_active_bytes: null,
      allocator_resident_bytes: null,
    },
    stats: { instantaneous_ops_per_sec: 0, expired_keys: 0, evicted_keys: 0, hit_rate: 1 },
    persistence: {
      loading: false,
      rdb_last_save_time: null,
      rdb_changes_since_last_save: 0,
      aof_enabled: false,
      aof_rewrite_in_progress: false,
    },
    replication: {
      role: "master",
      connected_replicas: 0,
      master_link_status: null,
      master_repl_offset: null,
    },
    command_stats: [],
  });
  getDatabaseOverviewMock.mockResolvedValue([
    { database: 0, key_count: 1, expires: 0, avg_ttl_ms: 0 },
  ]);
  analyzeDatabaseMock.mockResolvedValue({
    database: 0,
    total_keys: { total: 0, observed: 0, types: [] },
    total_memory: { total: 0, observed: 0, types: [] },
    top_keys_by_memory: [],
    top_keys_by_length: [],
    top_namespaces_by_keys: [],
    top_namespaces_by_memory: [],
    expiration_groups: [],
    node_results: [], failed_nodes: [],
    progress: { scanned: 0, processed: 0, truncated: false },
  });
  selectDatabaseMock.mockResolvedValue(localProfile);
  getAppSettingsMock.mockResolvedValue({
    version: 1,
    theme: "system",
    result_format: "text",
    scan_count: 100,
    continue_on_error: false,
  });
  listQueryLibraryMock.mockResolvedValue([]);
  saveAppSettingsMock.mockResolvedValue({
    version: 1,
    theme: "dark",
    result_format: "text",
    scan_count: 100,
    continue_on_error: false,
  });
  saveQueryLibraryItemMock.mockResolvedValue({
    id: "query-1",
    name: "读取用户",
    command: "GET user:1",
    tags: ["用户"],
    updated_at: 1,
  });
  deleteQueryLibraryItemMock.mockResolvedValue(undefined);
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

describe("Redix 应用壳", () => {
  it("操作面板展示连接限制，未连接可通过键盘打开设置", () => {
    render(<App />);
    fireEvent.keyDown(window, { key: "2", ctrlKey: true });
    expect(screen.getByRole("button", { name: "连接管理" })).toHaveAttribute("aria-current", "page");
    fireEvent.click(screen.getByRole("button", { name: "快捷键与操作" }));
    expect(screen.getByRole("option", { name: /Browser/ })).toHaveAttribute("aria-disabled", "true");
    expect(screen.getByRole("option", { name: /Cluster 拓扑/ })).toHaveAttribute("aria-disabled", "true");
    const search = screen.getByRole("combobox", { name: "搜索操作" });
    fireEvent.change(search, { target: { value: "应用偏好" } });
    fireEvent.keyDown(search, { key: "Enter" });
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "设置" })).toHaveAttribute("aria-current", "page");
    expect(screen.getByRole("button", { name: "设置" })).toHaveFocus();
  });

  it("连接后快捷键导航与聚焦现有编辑器，面板关闭恢复焦点且不执行命令", async () => {
    listConnectionsMock.mockResolvedValue([localProfile]);
    render(<App />);
    fireEvent.click(await screen.findByRole("button", { name: "连接" }));
    await screen.findByRole("heading", { name: "数据浏览" });
    fireEvent.keyDown(window, { key: "3", metaKey: true });
    const command = await screen.findByRole("textbox", { name: "Redis 命令" });
    fireEvent.keyDown(window, { key: "F", metaKey: true, shiftKey: true });
    expect(command).toHaveFocus();
    fireEvent.change(command, { target: { value: "SET important value" } });
    fireEvent.keyDown(command, { key: "2", metaKey: true });
    expect(screen.getByRole("heading", { name: "Workbench" })).toBeInTheDocument();
    fireEvent.keyDown(command, { key: "k", metaKey: true });
    fireEvent.keyDown(screen.getByRole("combobox"), { key: "Escape" });
    expect(command).toHaveFocus();
    expect(command).toHaveValue("SET important value");
    fireEvent.keyDown(command, { key: "k", ctrlKey: true });
    const dialog = screen.getByRole("dialog");
    fireEvent.change(within(dialog).getByRole("combobox"), { target: { value: "保存查询" } });
    fireEvent.keyDown(within(dialog).getByRole("combobox"), { key: "Enter" });
    expect(screen.getByRole("heading", { name: "Query Library" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Query Library" })).toHaveFocus();
    expect(executeCommandMock).not.toHaveBeenCalled();
    expect(executeCommandsMock).not.toHaveBeenCalled();
  });

  it("启动时显示独立连接管理页和应用名称", () => {
    render(<App />);

    expect(screen.getByRole("heading", { name: "Redix" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "连接管理" })).toBeInTheDocument();
    expect(screen.queryByRole("complementary", { name: "产品侧边栏" })).not.toBeInTheDocument();
    expect(screen.queryByText(/当前连接：|请先连接 Redis 后使用工作区/)).not.toBeInTheDocument();
    expect(openConnectionMock).not.toHaveBeenCalled();
  });

  it("连接管理页只提供本地功能入口", () => {
    render(<App />);

    const navigation = screen.getByRole("navigation", { name: "主导航" });
    expect(navigation).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "连接管理" })).toHaveAttribute(
      "aria-current",
      "page",
    );
    expect(screen.queryByRole("button", { name: "Browser" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Workbench" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Database" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "数据库分析" })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Query Library" })).toBeEnabled();
    expect(screen.getByRole("button", { name: "设置" })).toBeEnabled();
  });

  it("连接成功后进入对应工作区，返回连接管理后可重新进入", async () => {
    listConnectionsMock.mockResolvedValue([localProfile]);
    render(<App />);

    fireEvent.click(await screen.findByRole("button", { name: "连接" }));
    await screen.findByRole("heading", { name: "数据浏览" });
    expect(screen.getByRole("complementary", { name: "产品侧边栏" })).toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: "连接管理" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "连接管理" })).not.toBeInTheDocument();
    expect(screen.getByText(/当前连接：/)).toHaveTextContent("本地 Redis · 127.0.0.1:6379");

    fireEvent.click(screen.getByRole("button", { name: "返回连接管理" }));
    await screen.findByText("已连接");
    expect(screen.getByRole("heading", { name: "连接管理" })).toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: "数据浏览" })).not.toBeInTheDocument();
    expect(screen.queryByRole("complementary", { name: "产品侧边栏" })).not.toBeInTheDocument();
    expect(closeConnectionMock).not.toHaveBeenCalled();

    fireEvent.keyDown(window, { key: "2", ctrlKey: true });
    expect(screen.queryByRole("heading", { name: "数据浏览" })).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "重新连接" }));
    await screen.findByRole("heading", { name: "数据浏览" });
    expect(openConnectionMock).toHaveBeenCalledTimes(2);
  });

  it.each(["Workbench", "设置"])("切换到 %s 再返回时保留全量键列表、筛选和选择且不重复扫描", async (section) => {
    const summary = { key: "user:1", key_type: "string", ttl_ms: -1, size: 5 };
    listConnectionsMock.mockResolvedValue([localProfile]);
    scanAllKeysMock.mockImplementation(async ({ pattern }) => pattern === "*"
      ? [summary]
      : [summary, { ...summary, key: "user:profile", key_type: "hash" }, { ...summary, key: "user:2" }, { ...summary, key: "user:3" }]);
    render(<App />);
    fireEvent.click(await screen.findByRole("button", { name: "连接" }));
    await screen.findByRole("button", { name: "展开前缀 user:" });
    fireEvent.click(screen.getByRole("button", { name: "平铺" }));
    fireEvent.click(screen.getByRole("button", { name: "筛选" }));
    const pattern = screen.getByLabelText("键过滤");
    fireEvent.change(pattern, { target: { value: "user:*" } });
    fireEvent.keyDown(pattern, { key: "Enter" });
    await screen.findByRole("button", { name: "user:profile" });
    expect(screen.getByRole("button", { name: "user:3" })).toBeEnabled();
    expect(screen.queryByRole("button", { name: "加载更多" })).not.toBeInTheDocument();
    fireEvent.change(screen.getByLabelText("类型过滤"), { target: { value: "string" } });
    fireEvent.click(screen.getByRole("button", { name: "关闭筛选" }));
    fireEvent.click(screen.getByRole("checkbox", { name: "选择键 user:2" }));
    expect(scanAllKeysMock).toHaveBeenCalledTimes(2);
    expect(scanAllKeysMock).toHaveBeenLastCalledWith({
      connection_id: "local", pattern: "user:*", count: 100, key_type: null,
    });

    fireEvent.click(screen.getByRole("button", { name: section }));
    expect(screen.queryByRole("heading", { name: "数据浏览" })).not.toBeInTheDocument();
    if (section === "Workbench") {
      await screen.findByRole("textbox", { name: "Redis 命令" });
    } else {
      await screen.findByRole("button", { name: "保存设置" });
    }
    fireEvent.click(screen.getByRole("button", { name: "Browser" }));
    await screen.findByRole("heading", { name: "数据浏览" });

    expect(scanAllKeysMock).toHaveBeenCalledTimes(2);
    expect(screen.getByRole("button", { name: "平铺" })).toHaveAttribute("aria-pressed", "true");
    expect(screen.getByRole("button", { name: "user:1" })).toBeEnabled();
    expect(screen.getByRole("button", { name: "user:2" })).toBeEnabled();
    expect(screen.getByRole("button", { name: "user:3" })).toBeEnabled();
    expect(screen.queryByRole("button", { name: "user:profile" })).not.toBeInTheDocument();
    expect(screen.getByRole("checkbox", { name: "选择键 user:2" })).toBeChecked();
    expect(screen.getByRole("button", { name: "批量删除（1）" })).toBeEnabled();
    expect(screen.queryByRole("button", { name: "加载更多" })).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "筛选" }));
    expect(screen.getByLabelText("键过滤")).toHaveValue("user:*");
    expect(screen.getByLabelText("类型过滤")).toHaveValue("string");
    fireEvent.click(screen.getByRole("button", { name: "关闭筛选" }));
  });

  it("全量扫描期间禁止切库，离开 Browser 后完成的结果仍一次保留", async () => {
    let finishScan!: (keys: KeySummary[]) => void;
    const scanning = new Promise<KeySummary[]>((resolve) => { finishScan = resolve; });
    listConnectionsMock.mockResolvedValue([localProfile]);
    scanAllKeysMock.mockReturnValue(scanning);
    render(<App />);
    fireEvent.click(await screen.findByRole("button", { name: "连接" }));
    await waitFor(() => expect(scanAllKeysMock).toHaveBeenCalledTimes(1));

    expect(screen.getByRole("combobox", { name: "切换数据库" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "刷新键列表" })).toBeDisabled();
    expect(screen.queryByText("没有匹配的键。")).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "first-key" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "last-key" })).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Workbench" }));
    await screen.findByRole("textbox", { name: "Redis 命令" });

    await act(async () => {
      finishScan([
        { key: "first-key", key_type: "string", ttl_ms: -1, size: 5 },
        { key: "last-key", key_type: "hash", ttl_ms: -1, size: 2 },
      ]);
      await scanning;
    });
    fireEvent.click(screen.getByRole("button", { name: "Browser" }));

    expect(await screen.findByRole("button", { name: "first-key" })).toBeEnabled();
    expect(screen.getByRole("button", { name: "last-key" })).toBeEnabled();
    expect(screen.getByRole("combobox", { name: "切换数据库" })).toBeEnabled();
    expect(screen.queryByRole("button", { name: "加载更多" })).not.toBeInTheDocument();
    expect(scanAllKeysMock).toHaveBeenCalledTimes(1);
    expect(selectDatabaseMock).not.toHaveBeenCalled();
  });

  it("切换逻辑数据库后重新加载对应键列表并清除旧列表状态", async () => {
    let database = 0;
    listConnectionsMock.mockResolvedValue([localProfile]);
    getDatabaseOverviewMock.mockResolvedValue([
      { database: 0, key_count: 1, expires: 0, avg_ttl_ms: 0 },
      { database: 1, key_count: 1, expires: 0, avg_ttl_ms: 0 },
    ]);
    scanAllKeysMock.mockImplementation(async () => [
      { key: database === 0 ? "db-zero" : "db-one", key_type: "string", ttl_ms: -1, size: 5 },
    ]);
    selectDatabaseMock.mockImplementation(async (input) => {
      database = input.database;
      return { ...localProfile, database };
    });
    render(<App />);
    fireEvent.click(await screen.findByRole("button", { name: "连接" }));
    await screen.findByRole("button", { name: "db-zero" });
    fireEvent.click(screen.getByRole("checkbox", { name: "选择键 db-zero" }));
    fireEvent.click(screen.getByRole("button", { name: "筛选" }));
    fireEvent.change(screen.getByLabelText("类型过滤"), { target: { value: "string" } });
    fireEvent.click(screen.getByRole("button", { name: "关闭筛选" }));

    fireEvent.click(screen.getByRole("button", { name: "Database" }));
    fireEvent.click(await screen.findByRole("button", { name: "切换到数据库 1" }));
    await screen.findByText("当前数据库：1");
    fireEvent.click(screen.getByRole("button", { name: "Browser" }));

    expect(await screen.findByRole("button", { name: "db-one" })).toBeEnabled();
    expect(screen.queryByRole("button", { name: "db-zero" })).not.toBeInTheDocument();
    expect(screen.getByRole("checkbox", { name: "选择键 db-one" })).not.toBeChecked();
    expect(screen.getByRole("button", { name: "批量删除" })).toBeDisabled();
    expect(screen.queryByRole("button", { name: "加载更多" })).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "筛选" }));
    expect(screen.getByLabelText("键过滤")).toHaveValue("*");
    expect(screen.getByLabelText("类型过滤")).toHaveValue("");
    expect(selectDatabaseMock).toHaveBeenCalledWith({ connection_id: "local", database: 1 });
    expect(scanAllKeysMock).toHaveBeenCalledTimes(2);
    expect(scanAllKeysMock).toHaveBeenLastCalledWith({
      connection_id: "local", pattern: "*", count: 100, key_type: null,
    });
  });

  it("在 Browser 直接切换 Db 后清除旧选择并同步数据库概览", async () => {
    let database = 0;
    listConnectionsMock.mockResolvedValue([localProfile]);
    scanAllKeysMock.mockImplementation(async () => [
      { key: database === 0 ? "db-zero" : "db-one", key_type: "string", ttl_ms: -1, size: 5 },
    ]);
    selectDatabaseMock.mockImplementation(async (input) => {
      database = input.database;
      return { ...localProfile, database };
    });
    render(<App />);
    fireEvent.click(await screen.findByRole("button", { name: "连接" }));
    await screen.findByRole("button", { name: "db-zero" });
    fireEvent.click(screen.getByRole("checkbox", { name: "选择键 db-zero" }));
    fireEvent.click(screen.getByRole("button", { name: "筛选" }));
    fireEvent.change(screen.getByLabelText("类型过滤"), { target: { value: "string" } });
    fireEvent.click(screen.getByRole("button", { name: "关闭筛选" }));

    fireEvent.change(screen.getByRole("combobox", { name: "切换数据库" }), { target: { value: "1" } });

    expect(await screen.findByRole("button", { name: "db-one" })).toBeEnabled();
    expect(screen.getByRole("combobox", { name: "切换数据库" })).toHaveValue("1");
    expect(screen.queryByRole("button", { name: "db-zero" })).not.toBeInTheDocument();
    expect(screen.getByRole("checkbox", { name: "选择键 db-one" })).not.toBeChecked();
    expect(screen.getByRole("button", { name: "批量删除" })).toBeDisabled();
    expect(screen.queryByRole("button", { name: "加载更多" })).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "筛选" }));
    expect(screen.getByLabelText("类型过滤")).toHaveValue("");
    fireEvent.click(screen.getByRole("button", { name: "关闭筛选" }));
    expect(selectDatabaseMock).toHaveBeenCalledWith({ connection_id: "local", database: 1 });
    expect(scanAllKeysMock).toHaveBeenCalledTimes(2);
    expect(scanAllKeysMock).toHaveBeenLastCalledWith({ connection_id: "local", pattern: "*", count: 100, key_type: null });
    fireEvent.click(screen.getByRole("button", { name: "Database" }));
    expect(await screen.findByText("当前数据库：1")).toBeInTheDocument();
  });

  it("Browser 切换 Db 期间禁用键操作，失败后保留原数据库和选择", async () => {
    let rejectSwitch!: (reason: unknown) => void;
    selectDatabaseMock.mockReturnValue(new Promise((_, reject) => { rejectSwitch = reject; }));
    listConnectionsMock.mockResolvedValue([localProfile]);
    scanAllKeysMock.mockResolvedValue([{ key: "db-zero", key_type: "string", ttl_ms: -1, size: 5 }]);
    render(<App />);
    fireEvent.click(await screen.findByRole("button", { name: "连接" }));
    await screen.findByRole("button", { name: "db-zero" });
    fireEvent.click(screen.getByRole("checkbox", { name: "选择键 db-zero" }));

    fireEvent.change(screen.getByRole("combobox", { name: "切换数据库" }), { target: { value: "1" } });

    expect(screen.getByRole("combobox", { name: "切换数据库" })).toBeDisabled();
    for (const name of ["新增键", "刷新键列表", "批量删除（1）", "导出选中键", "筛选", "db-zero"]) {
      expect(screen.getByRole("button", { name })).toBeDisabled();
    }
    expect(screen.getByLabelText("导入 JSON 文件")).toBeDisabled();
    expect(scanAllKeysMock).toHaveBeenCalledTimes(1);

    await act(async () => { rejectSwitch(new Error("DB index is out of range")); });

    expect(await screen.findByRole("alert")).toHaveTextContent("数据库切换失败");
    expect(screen.getByRole("combobox", { name: "切换数据库" })).toBeEnabled();
    expect(screen.getByRole("combobox", { name: "切换数据库" })).toHaveValue("0");
    expect(screen.getByRole("checkbox", { name: "选择键 db-zero" })).toBeChecked();
    expect(screen.getByRole("button", { name: "批量删除（1）" })).toBeEnabled();
    expect(scanAllKeysMock).toHaveBeenCalledTimes(1);
  });

  it("导入文件尚未读取完成时禁用 Browser 数据库切换", async () => {
    let readFile!: (text: string) => void;
    const file = new File(["[]"], "keys.json", { type: "application/json" });
    Object.defineProperty(file, "text", { value: () => new Promise<string>((resolve) => { readFile = resolve; }) });
    listConnectionsMock.mockResolvedValue([localProfile]);
    render(<App />);
    fireEvent.click(await screen.findByRole("button", { name: "连接" }));
    await screen.findByText("没有匹配的键。");
    fireEvent.change(screen.getByLabelText("导入 JSON 文件"), { target: { files: [file] } });

    expect(screen.getByRole("combobox", { name: "切换数据库" })).toBeDisabled();

    await act(async () => { readFile("[]"); });
    expect(screen.getByRole("combobox", { name: "切换数据库" })).toBeEnabled();
    expect(selectDatabaseMock).not.toHaveBeenCalled();
  });

  it("筛选防抖尚未执行时禁止切库，完成筛选后恢复", async () => {
    listConnectionsMock.mockResolvedValue([localProfile]);
    render(<App />);
    fireEvent.click(await screen.findByRole("button", { name: "连接" }));
    await screen.findByText("没有匹配的键。");
    fireEvent.click(screen.getByRole("button", { name: "筛选" }));
    fireEvent.change(screen.getByLabelText("键过滤"), { target: { value: "user:*" } });
    fireEvent.click(screen.getByRole("button", { name: "关闭筛选" }));

    expect(screen.getByRole("combobox", { name: "切换数据库" })).toBeDisabled();
    await waitFor(() => expect(scanAllKeysMock).toHaveBeenCalledTimes(2));
    expect(scanAllKeysMock).toHaveBeenLastCalledWith({ connection_id: "local", pattern: "user:*", count: 100, key_type: null });
    expect(screen.getByRole("combobox", { name: "切换数据库" })).toBeEnabled();
  });

  it("数据库切换尚未完成就返回 Browser，完成后仍加载新数据库并清除旧选择", async () => {
    let database = 0;
    let finishSwitch!: (profile: typeof localProfile) => void;
    const switching = new Promise<typeof localProfile>((resolve) => { finishSwitch = resolve; });
    listConnectionsMock.mockResolvedValue([localProfile]);
    getDatabaseOverviewMock.mockResolvedValue([
      { database: 0, key_count: 1, expires: 0, avg_ttl_ms: 0 },
      { database: 1, key_count: 1, expires: 0, avg_ttl_ms: 0 },
    ]);
    scanAllKeysMock.mockImplementation(async () => [
      { key: database === 0 ? "db-zero" : "db-one", key_type: "string", ttl_ms: -1, size: 5 },
    ]);
    selectDatabaseMock.mockReturnValue(switching);
    render(<App />);
    fireEvent.click(await screen.findByRole("button", { name: "连接" }));
    await screen.findByRole("button", { name: "db-zero" });
    fireEvent.click(screen.getByRole("checkbox", { name: "选择键 db-zero" }));

    fireEvent.click(screen.getByRole("button", { name: "Database" }));
    fireEvent.click(await screen.findByRole("button", { name: "切换到数据库 1" }));
    expect(selectDatabaseMock).toHaveBeenCalledWith({ connection_id: "local", database: 1 });
    fireEvent.click(screen.getByRole("button", { name: "Browser" }));
    expect(screen.getByRole("checkbox", { name: "选择键 db-zero" })).toBeChecked();
    expect(screen.getByRole("combobox", { name: "切换数据库" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "刷新键列表" })).toBeDisabled();
    expect(scanAllKeysMock).toHaveBeenCalledTimes(1);

    await act(async () => {
      database = 1;
      finishSwitch({ ...localProfile, database: 1 });
      await switching;
    });

    expect(scanAllKeysMock).toHaveBeenCalledTimes(2);
    expect(await screen.findByRole("button", { name: "db-one" })).toBeEnabled();
    expect(screen.queryByRole("button", { name: "db-zero" })).not.toBeInTheDocument();
    expect(screen.getByRole("checkbox", { name: "选择键 db-one" })).not.toBeChecked();
    expect(screen.getByRole("button", { name: "批量删除" })).toBeDisabled();
    expect(scanAllKeysMock).toHaveBeenLastCalledWith({
      connection_id: "local", pattern: "*", count: 100, key_type: null,
    });
  });

  it("Browser 切库尚未完成时，Database 页面不能发起另一次切库", async () => {
    let finishSwitch!: (profile: typeof localProfile) => void;
    selectDatabaseMock.mockReturnValue(new Promise<typeof localProfile>((resolve) => { finishSwitch = resolve; }));
    listConnectionsMock.mockResolvedValue([localProfile]);
    getDatabaseOverviewMock.mockResolvedValue([
      { database: 0, key_count: 1, expires: 0, avg_ttl_ms: 0 },
      { database: 1, key_count: 1, expires: 0, avg_ttl_ms: 0 },
      { database: 2, key_count: 1, expires: 0, avg_ttl_ms: 0 },
    ]);
    render(<App />);
    fireEvent.click(await screen.findByRole("button", { name: "连接" }));
    await screen.findByText("没有匹配的键。");
    fireEvent.change(screen.getByRole("combobox", { name: "切换数据库" }), { target: { value: "1" } });
    fireEvent.click(screen.getByRole("button", { name: "Database" }));

    expect(await screen.findByRole("button", { name: "切换到数据库 2" })).toBeDisabled();

    await act(async () => { finishSwitch({ ...localProfile, database: 1 }); });
    expect(screen.getByText("当前数据库：1")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "切换到数据库 2" })).toBeEnabled();
    expect(selectDatabaseMock).toHaveBeenCalledTimes(1);
  });

  it("从其他工作区也能通过快捷键返回，切换连接后使用新连接", async () => {
    const remoteProfile = { ...localProfile, id: "remote", name: "远程 Redis", host: "192.0.2.10" };
    listConnectionsMock.mockResolvedValue([localProfile, remoteProfile]);
    scanAllKeysMock.mockImplementation(async ({ connection_id }) => [
      { key: connection_id === "local" ? "local-key" : "remote-key", key_type: "string", ttl_ms: -1, size: 5 },
    ]);
    render(<App />);

    fireEvent.click((await screen.findAllByRole("button", { name: "连接" }))[0]);
    await screen.findByRole("button", { name: "local-key" });
    fireEvent.click(screen.getByRole("button", { name: "Workbench" }));
    await screen.findByRole("textbox", { name: "Redis 命令" });
    fireEvent.keyDown(window, { key: "1", metaKey: true });
    await screen.findByText("远程 Redis");
    expect(screen.queryByRole("complementary", { name: "产品侧边栏" })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "连接管理" })).toHaveFocus();

    fireEvent.click(screen.getByRole("button", { name: "连接" }));
    await screen.findByRole("heading", { name: "数据浏览" });
    expect(closeConnectionMock).toHaveBeenCalledWith("local");
    expect(screen.getByText(/当前连接：/)).toHaveTextContent("远程 Redis · 192.0.2.10:6379");
    expect(await screen.findByRole("button", { name: "remote-key" })).toBeEnabled();
    expect(screen.queryByRole("button", { name: "local-key" })).not.toBeInTheDocument();
    await waitFor(() => expect(scanAllKeysMock).toHaveBeenLastCalledWith(expect.objectContaining({ connection_id: "remote" })));
  });

  it("切换连接后忽略旧连接迟到的全量扫描结果", async () => {
    let finishLocalScan!: (keys: KeySummary[]) => void;
    const localScan = new Promise<KeySummary[]>((resolve) => { finishLocalScan = resolve; });
    const remoteProfile = { ...localProfile, id: "remote", name: "远程 Redis", host: "192.0.2.10" };
    listConnectionsMock.mockResolvedValue([localProfile, remoteProfile]);
    scanAllKeysMock.mockImplementation(async ({ connection_id }) => connection_id === "local"
      ? localScan
      : [{ key: "remote-key", key_type: "string", ttl_ms: -1, size: 5 }]);
    render(<App />);
    fireEvent.click((await screen.findAllByRole("button", { name: "连接" }))[0]);
    await waitFor(() => expect(scanAllKeysMock).toHaveBeenCalledTimes(1));
    fireEvent.click(screen.getByRole("button", { name: "返回连接管理" }));
    fireEvent.click(await screen.findByRole("button", { name: "连接" }));
    await screen.findByRole("button", { name: "remote-key" });
    fireEvent.click(screen.getByRole("checkbox", { name: "选择键 remote-key" }));

    await act(async () => {
      finishLocalScan([{ key: "late-local-key", key_type: "string", ttl_ms: -1, size: 5 }]);
      await localScan;
    });

    expect(screen.getByText(/当前连接：/)).toHaveTextContent("远程 Redis");
    expect(screen.getByRole("button", { name: "remote-key" })).toBeEnabled();
    expect(screen.getByRole("checkbox", { name: "选择键 remote-key" })).toBeChecked();
    expect(screen.queryByRole("button", { name: "late-local-key" })).not.toBeInTheDocument();
    expect(scanAllKeysMock).toHaveBeenCalledTimes(2);
    expect(scanAllKeysMock).toHaveBeenLastCalledWith({
      connection_id: "remote", pattern: "*", count: 100, key_type: null,
    });
  });

  it("连接失败时停留在连接管理页，允许重试", async () => {
    listConnectionsMock.mockResolvedValue([localProfile]);
    openConnectionMock.mockRejectedValueOnce({ code: "CONNECTION_FAILED" });
    render(<App />);

    fireEvent.click(await screen.findByRole("button", { name: "连接" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("无法连接到 Redis 服务器");
    expect(screen.getByRole("heading", { name: "连接管理" })).toBeInTheDocument();
    expect(screen.queryByRole("complementary", { name: "产品侧边栏" })).not.toBeInTheDocument();
    expect(scanAllKeysMock).not.toHaveBeenCalled();

    fireEvent.click(screen.getByRole("button", { name: "连接" }));
    await screen.findByRole("heading", { name: "数据浏览" });
  });

  it("返回后删除当前连接会清除连接上下文", async () => {
    const remoteProfile = { ...localProfile, id: "remote", name: "远程 Redis" };
    listConnectionsMock.mockResolvedValue([localProfile, remoteProfile]);
    vi.stubGlobal("confirm", vi.fn().mockReturnValue(false));
    render(<App />);

    fireEvent.click((await screen.findAllByRole("button", { name: "连接" }))[0]);
    await screen.findByRole("heading", { name: "数据浏览" });
    fireEvent.click(screen.getByRole("button", { name: "返回连接管理" }));
    fireEvent.click(await screen.findByRole("button", { name: "删除 本地 Redis" }));
    expect(deleteConnectionMock).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "确认删除" }));
    await waitFor(() => expect(screen.queryByText("本地 Redis")).not.toBeInTheDocument());
    expect(deleteConnectionMock).toHaveBeenCalledWith("local");
    expect(screen.queryByText("已连接")).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "连接" }));
    await screen.findByRole("heading", { name: "数据浏览" });
    expect(closeConnectionMock).not.toHaveBeenCalled();
    expect(screen.getByText(/当前连接：/)).toHaveTextContent("远程 Redis");
  });

  it("在连接管理的查询库选择命令后，连接并进入 Workbench 仍可回填", async () => {
    listConnectionsMock.mockResolvedValue([localProfile]);
    listQueryLibraryMock.mockResolvedValue([
      { id: "query-1", name: "读取用户", command: "GET user:1", tags: [], updated_at: 1 },
    ]);
    render(<App />);
    fireEvent.click(screen.getByRole("button", { name: "Query Library" }));
    fireEvent.click(await screen.findByRole("button", { name: "回填 Workbench 读取用户" }));
    fireEvent.click(await screen.findByRole("button", { name: "连接" }));
    await screen.findByRole("heading", { name: "数据浏览" });
    fireEvent.click(screen.getByRole("button", { name: "Workbench" }));
    expect(await screen.findByRole("textbox", { name: "Redis 命令" })).toHaveValue("GET user:1");
    expect(executeCommandMock).not.toHaveBeenCalled();
  });

  it("连接后显示可用的数据库分析入口，默认仍停留在 Browser", async () => {
    listConnectionsMock.mockResolvedValue([localProfile]);
    render(<App />);

    fireEvent.click(await screen.findByRole("button", { name: "连接" }));
    await waitFor(() => expect(openConnectionMock).toHaveBeenCalledWith("local"));

    expect(screen.getByRole("button", { name: "数据库分析" })).toBeEnabled();
    expect(screen.getByRole("heading", { name: "数据浏览" })).toBeInTheDocument();
    expect(analyzeDatabaseMock).not.toHaveBeenCalled();
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

  it("连接后显示 RedisSearch / Query 工作区", async () => {
    listConnectionsMock.mockResolvedValue([localProfile]);
    getModuleCapabilitiesMock.mockResolvedValue({
      modules: [{ name: "search", version: "2.8.10" }],
      json_supported: false,
      json_version: null,
      search_supported: true,
      search_version: "2.8.10",
      array_supported: false,
      vector_set_supported: false,
    });
    listSearchIndexesMock.mockResolvedValue({ indexes: [] });

    render(<App />);
    fireEvent.click(await screen.findByRole("button", { name: "连接" }));
    await waitFor(() => expect(openConnectionMock).toHaveBeenCalledWith("local"));
    fireEvent.click(screen.getByRole("button", { name: "Search / Query" }));

    expect(await screen.findByRole("heading", { name: "RedisSearch / Query" })).toBeInTheDocument();
    expect(listSearchIndexesMock).toHaveBeenCalledWith("local");
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

  it("Query Library 回填 Workbench，不自动执行命令", async () => {
    listConnectionsMock.mockResolvedValue([localProfile]);
    listQueryLibraryMock.mockResolvedValue([
      {
        id: "query-1",
        name: "读取用户",
        command: "GET user:1",
        tags: ["用户"],
        updated_at: 1,
      },
    ]);
    render(<App />);

    fireEvent.click(await screen.findByRole("button", { name: "连接" }));
    await waitFor(() => expect(openConnectionMock).toHaveBeenCalledWith("local"));
    fireEvent.click(screen.getByRole("button", { name: "Query Library" }));
    await screen.findByText("读取用户");
    fireEvent.click(screen.getByRole("button", { name: "回填 Workbench 读取用户" }));

    expect(screen.getByRole("heading", { name: "Workbench" })).toBeInTheDocument();
    expect(screen.getByRole("textbox", { name: "Redis 命令" })).toHaveValue("GET user:1");
    expect(executeCommandMock).not.toHaveBeenCalled();
  });

  it("保存设置后更新应用主题属性", async () => {
    render(<App />);
    fireEvent.click(screen.getByRole("button", { name: "设置" }));
    fireEvent.change(screen.getByLabelText("主题"), { target: { value: "dark" } });
    fireEvent.click(screen.getByRole("button", { name: "保存设置" }));

    await waitFor(() => expect(saveAppSettingsMock).toHaveBeenCalledTimes(1));
    await waitFor(() =>
      expect(document.documentElement).toHaveAttribute("data-theme", "dark"),
    );
  });
});
