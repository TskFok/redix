import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import {
  getDatabaseOverview,
  getInstanceDetails,
  selectDatabase,
} from "../../lib/tauri";
import type {
  ConnectionProfile,
  DatabaseOverview,
  InstanceDetails,
  InstanceOverview,
} from "../../lib/types";
import DatabasePage from "./DatabasePage";

vi.mock("../../lib/tauri", () => ({
  getDatabaseOverview: vi.fn(),
  getInstanceDetails: vi.fn(),
  selectDatabase: vi.fn(),
}));

const getInstanceDetailsMock = vi.mocked(getInstanceDetails);
const getDatabaseOverviewMock = vi.mocked(getDatabaseOverview);
const selectDatabaseMock = vi.mocked(selectDatabase);

it("Cluster 仅取 DB0 聚合并提供拓扑入口", async () => {
  getDatabaseOverviewMock.mockResolvedValue([{ database: 0, key_count: 42, expires: 2, avg_ttl_ms: null }]);
  const open = vi.fn();
  render(<DatabasePage connectionId="cluster" activeDatabase={0} isCluster onOpenTopology={open} onProfileChanged={vi.fn()} />);
  expect(await screen.findByText("42")).toBeInTheDocument();
  expect(getInstanceDetailsMock).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("button", { name: "查看 Cluster 拓扑" }));
  expect(open).toHaveBeenCalledOnce();
});

const profile: ConnectionProfile = {
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

const instance: InstanceOverview = {
  server_version: "7.2.5",
  redis_mode: "standalone",
  uptime_seconds: 42,
  connected_clients: 3,
  used_memory_bytes: 1024,
  max_memory_bytes: null,
  total_commands_processed: 9,
  keyspace_hits: 4,
  keyspace_misses: 1,
  role: "master",
  modules: [],
};

const details: InstanceDetails = {
  overview: instance,
  clients: {
    connected_clients: 3,
    blocked_clients: 1,
    tracking_clients: 0,
    max_clients: 10_000,
  },
  memory: {
    used_memory_bytes: 1024,
    used_memory_peak_bytes: 2048,
    used_memory_rss_bytes: 4096,
    mem_fragmentation_ratio: 1.2,
    allocator_active_bytes: null,
    allocator_resident_bytes: null,
  },
  stats: {
    instantaneous_ops_per_sec: 12,
    expired_keys: 4,
    evicted_keys: 0,
    hit_rate: 0.8,
  },
  persistence: {
    loading: false,
    rdb_last_save_time: 1_710_000_000,
    rdb_changes_since_last_save: 2,
    aof_enabled: false,
    aof_rewrite_in_progress: false,
  },
  replication: {
    role: "master",
    connected_replicas: 0,
    master_link_status: null,
    master_repl_offset: null,
  },
  command_stats: [
    {
      command: "GET",
      calls: 4,
      usec: 20,
      usec_per_call: 5,
      rejected_calls: 0,
      failed_calls: 1,
    },
  ],
};

const databases: DatabaseOverview[] = [
  { database: 0, key_count: 8, expires: 2, avg_ttl_ms: 1200 },
  { database: 1, key_count: null, expires: null, avg_ttl_ms: null },
];

function renderPage(
  connectionId = "local",
  activeDatabase = 0,
  onProfileChanged = vi.fn(),
) {
  return render(
    <DatabasePage
      connectionId={connectionId}
      activeDatabase={activeDatabase}
      onProfileChanged={onProfileChanged}
    />,
  );
}

beforeEach(() => {
  vi.clearAllMocks();
  getInstanceDetailsMock.mockResolvedValue(details);
  getDatabaseOverviewMock.mockResolvedValue(databases);
  selectDatabaseMock.mockResolvedValue(profile);
});

afterEach(() => {
  cleanup();
});

describe("DatabasePage", () => {
  it("加载实例详情和数据库列表，并将空指标显示为不可用", async () => {
    renderPage();

    expect(await screen.findByRole("heading", { name: "数据库概览" })).toBeInTheDocument();
    expect(screen.getByText("7.2.5")).toBeInTheDocument();
    expect(screen.getByText("数据库 0")).toBeInTheDocument();
    expect(screen.getByText("数据库 1")).toBeInTheDocument();
    const clientPanel = screen.getByRole("heading", { name: "客户端" }).closest("section");
    expect(clientPanel).not.toBeNull();
    expect(within(clientPanel!).getByText("阻塞客户端")).toBeInTheDocument();
    expect(within(clientPanel!).getByText("1")).toBeInTheDocument();
    expect(screen.getByText("内存峰值")).toBeInTheDocument();
    expect(screen.getByText("2.0 KB")).toBeInTheDocument();
    expect(screen.getByText("80.0%")).toBeInTheDocument();
    expect(screen.getByRole("rowheader", { name: "GET" })).toBeInTheDocument();
    expect(screen.getByText("未检测到模块或模块信息不可用。")).toBeInTheDocument();
    expect(screen.getAllByText("不可用").length).toBeGreaterThanOrEqual(3);
  });

  it("切换数据库成功后只通过回调更新 profile", async () => {
    const onProfileChanged = vi.fn();
    renderPage("local", 0, onProfileChanged);

    await screen.findByText("数据库 1");
    fireEvent.click(screen.getByRole("button", { name: "切换到数据库 1" }));

    await waitFor(() => expect(selectDatabaseMock).toHaveBeenCalledTimes(1));
    expect(selectDatabaseMock).toHaveBeenCalledWith({
      connection_id: "local",
      database: 1,
    });
    expect(onProfileChanged).toHaveBeenCalledWith(profile);
  });

  it("切换失败时保留原数据库并显示固定错误", async () => {
    selectDatabaseMock.mockRejectedValueOnce({
      code: "COMMAND_FAILED",
      message: "不应直接显示的 Redis 错误",
    });
    renderPage("local", 0);

    await screen.findByText("数据库 1");
    fireEvent.click(screen.getByRole("button", { name: "切换到数据库 1" }));

    expect(await screen.findByText("数据库切换失败，请稍后重试。")).toBeInTheDocument();
    expect(screen.getByText("当前数据库：0")).toBeInTheDocument();
  });

  it("连接变化后忽略旧详情和数据库列表响应", async () => {
    let resolveOldDetails: (value: InstanceDetails) => void = () => undefined;
    let resolveOldDatabases: (value: DatabaseOverview[]) => void = () => undefined;
    const oldDetails = new Promise<InstanceDetails>((resolve) => {
      resolveOldDetails = resolve;
    });
    const oldDatabases = new Promise<DatabaseOverview[]>((resolve) => {
      resolveOldDatabases = resolve;
    });
    getInstanceDetailsMock.mockReturnValueOnce(oldDetails).mockResolvedValueOnce({
      ...details,
      overview: { ...instance, server_version: "8.0.0" },
    });
    getDatabaseOverviewMock.mockReturnValueOnce(oldDatabases).mockResolvedValueOnce([
      { database: 2, key_count: 2, expires: 0, avg_ttl_ms: 0 },
    ]);

    const view = renderPage("old");
    view.rerender(
      <DatabasePage
        connectionId="new"
        activeDatabase={2}
        onProfileChanged={vi.fn()}
      />,
    );
    expect(await screen.findByText("8.0.0")).toBeInTheDocument();

    await act(async () => {
      resolveOldDetails({ ...details, overview: { ...instance, server_version: "old" } });
      resolveOldDatabases(databases);
    });
    expect(screen.queryByText("old")).not.toBeInTheDocument();
    expect(screen.getByText("8.0.0")).toBeInTheDocument();
  });
});
