import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import {
  getDatabaseOverview,
  getInstanceOverview,
  selectDatabase,
} from "../../lib/tauri";
import type { ConnectionProfile, DatabaseOverview, InstanceOverview } from "../../lib/types";
import DatabasePage from "./DatabasePage";

vi.mock("../../lib/tauri", () => ({
  getDatabaseOverview: vi.fn(),
  getInstanceOverview: vi.fn(),
  selectDatabase: vi.fn(),
}));

const getInstanceOverviewMock = vi.mocked(getInstanceOverview);
const getDatabaseOverviewMock = vi.mocked(getDatabaseOverview);
const selectDatabaseMock = vi.mocked(selectDatabase);

const profile: ConnectionProfile = {
  id: "local",
  name: "本地 Redis",
  host: "127.0.0.1",
  port: 6379,
  username: null,
  database: 0,
  has_password: false,
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
  getInstanceOverviewMock.mockResolvedValue(instance);
  getDatabaseOverviewMock.mockResolvedValue(databases);
  selectDatabaseMock.mockResolvedValue(profile);
});

afterEach(() => {
  cleanup();
});

describe("DatabasePage", () => {
  it("加载实例指标和数据库列表，并将空指标显示为不可用", async () => {
    renderPage();

    expect(await screen.findByRole("heading", { name: "数据库概览" })).toBeInTheDocument();
    expect(screen.getByText("7.2.5")).toBeInTheDocument();
    expect(screen.getByText("3")).toBeInTheDocument();
    expect(screen.getByText("数据库 0")).toBeInTheDocument();
    expect(screen.getByText("数据库 1")).toBeInTheDocument();
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

  it("连接变化后忽略旧概览响应", async () => {
    let resolveOldInstance: (value: InstanceOverview) => void = () => undefined;
    let resolveOldDatabases: (value: DatabaseOverview[]) => void = () => undefined;
    const oldInstance = new Promise<InstanceOverview>((resolve) => {
      resolveOldInstance = resolve;
    });
    const oldDatabases = new Promise<DatabaseOverview[]>((resolve) => {
      resolveOldDatabases = resolve;
    });
    getInstanceOverviewMock.mockReturnValueOnce(oldInstance).mockResolvedValueOnce({
      ...instance,
      server_version: "8.0.0",
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
      resolveOldInstance({ ...instance, server_version: "old" });
      resolveOldDatabases(databases);
    });
    expect(screen.queryByText("old")).not.toBeInTheDocument();
    expect(screen.getByText("8.0.0")).toBeInTheDocument();
  });
});
