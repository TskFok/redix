import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  within,
} from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import type { ClusterTopology } from "../../lib/types";
import TopologyPage from "./TopologyPage";

const { get, refresh } = vi.hoisted(() => ({ get: vi.fn(), refresh: vi.fn() }));
vi.mock("../../lib/tauri", () => ({
  getClusterTopology: get,
  refreshClusterTopology: refresh,
}));
const topology: ClusterTopology = {
  summary: {
    state: "ok",
    slots_assigned: 16384,
    slots_ok: 16384,
    slots_pfail: 0,
    slots_fail: 0,
    current_epoch: 2,
    size: 1,
    known_nodes: 2,
  },
  failures: [{ node_id: "node-2", code: "CONNECTION_FAILED" }],
  nodes: [
    {
      id: "node-1",
      endpoint: { host: "::1", port: 7000 },
      connection_endpoint: null,
      role: "primary",
      health: "online",
      primary_id: null,
      slots: [{ start: 0, end: 16383 }],
      metrics: {
        server_version: null,
        redis_mode: null,
        total_keys: null,
        maxmemory_bytes: null,
        used_memory_bytes: 2048,
        ops_per_second: 42,
        connected_clients: 3,
        connections_received: 4,
        commands_processed: 5,
        network_in_kbps: null,
        network_out_kbps: null,
        cache_hit_ratio: null,
        replication_offset: 10,
        replication_lag: null,
        uptime_seconds: 60,
      },
    },
  ],
};
beforeEach(() => {
  vi.resetAllMocks();
  get.mockResolvedValue(topology);
  refresh.mockResolvedValue({ ...topology, failures: [] });
});
afterEach(cleanup);
it("显示健康节点及部分失败，显式刷新更新结果", async () => {
  render(<TopologyPage connectionId="cluster" />);
  expect(await screen.findByText("node-1")).toBeInTheDocument();
  expect(screen.getByRole("alert")).toHaveTextContent("1 个节点暂不可用");
  expect(screen.getByText("[::1]:7000")).toBeInTheDocument();
  expect(screen.getByText("42")).toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "刷新拓扑" }));
  await act(async () => {});
  expect(refresh).toHaveBeenCalledWith("cluster");
  expect(screen.queryByRole("alert")).not.toBeInTheDocument();
});

it("节点展示版本、运行模式、键总量和内存上限，并区分零值与缺失", async () => {
  get.mockResolvedValue({ ...topology, nodes: [{ ...topology.nodes[0], metrics: {
    ...topology.nodes[0].metrics,
    server_version: "8.2.1", redis_mode: "cluster", total_keys: 15, maxmemory_bytes: 0,
  } }, { ...topology.nodes[0], id: "node-2", metrics: {
    ...topology.nodes[0].metrics,
    server_version: null, redis_mode: null, total_keys: null, maxmemory_bytes: null,
  } }] });
  render(<TopologyPage connectionId="cluster" />);
  await screen.findByText("node-1");
  const headers = screen.getAllByRole("columnheader");
  const present = within(screen.getByRole("rowheader", { name: "node-1" }).closest("tr")!);
  const missing = within(screen.getByRole("rowheader", { name: "node-2" }).closest("tr")!);
  for (const [label, expected] of [["服务端版本", "8.2.1"], ["运行模式", "cluster"], ["键总量", "15"], ["内存上限 (bytes)", "0（无限制）"]]) {
    const index = headers.indexOf(screen.getByRole("columnheader", { name: label })) - 1;
    expect(present.getAllByRole("cell")[index]).toHaveTextContent(expected);
    expect(missing.getAllByRole("cell")[index]).toHaveTextContent("不可用");
  }
});
it("切换连接后忽略旧请求及旧刷新结果", async () => {
  let finish: (value: ClusterTopology) => void = () => {};
  refresh.mockImplementation(
    () =>
      new Promise((resolve) => {
        finish = resolve;
      }),
  );
  const view = render(<TopologyPage connectionId="old" />);
  await screen.findByText("node-1");
  fireEvent.click(screen.getByRole("button", { name: "刷新拓扑" }));
  get.mockResolvedValue({ ...topology, nodes: [], failures: [] });
  view.rerender(<TopologyPage connectionId="new" />);
  await screen.findByText("暂无拓扑节点");
  await act(async () => {
    finish(topology);
  });
  expect(screen.queryByText("node-1")).not.toBeInTheDocument();
  view.unmount();
});
it("失败只显示稳定错误并可以重试", async () => {
  get.mockRejectedValue({ message: "secret-host-password" });
  render(<TopologyPage connectionId="cluster" />);
  expect(await screen.findByRole("alert")).toHaveTextContent(
    "无法加载 Cluster 拓扑",
  );
  expect(screen.queryByText(/secret-host-password/)).not.toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "刷新拓扑" }));
  expect(await screen.findByText("node-1")).toBeInTheDocument();
});

it("刷新失败保留旧结果并提示，加载期间阻止重复刷新", async () => {
  let fail: (reason: unknown) => void = () => {};
  refresh.mockImplementation(
    () =>
      new Promise((_, reject) => {
        fail = reject;
      }),
  );
  render(<TopologyPage connectionId="cluster" />);
  await screen.findByText("node-1");
  fireEvent.click(screen.getByRole("button", { name: "刷新拓扑" }));
  const busy = screen.getByRole("button", { name: "加载拓扑中…" });
  expect(busy).toBeDisabled();
  fireEvent.click(busy);
  expect(refresh).toHaveBeenCalledTimes(1);
  await act(async () => {
    fail(new Error("secret"));
  });
  expect(screen.getByText(/当前显示上次结果/)).toBeInTheDocument();
  expect(screen.getByText("node-1")).toBeInTheDocument();
});

it("切换连接和卸载后丢弃未完成的首次加载", async () => {
  let finish: (result: ClusterTopology) => void = () => {};
  get.mockImplementationOnce(
    () =>
      new Promise((resolve) => {
        finish = resolve;
      }),
  );
  const view = render(<TopologyPage connectionId="old" />);
  get.mockResolvedValue({ ...topology, nodes: [], failures: [] });
  view.rerender(<TopologyPage connectionId="new" />);
  await screen.findByText("暂无拓扑节点");
  await act(async () => {
    finish(topology);
  });
  expect(screen.queryByText("node-1")).not.toBeInTheDocument();
  let refreshed: (result: ClusterTopology) => void = () => {};
  refresh.mockImplementationOnce(
    () =>
      new Promise((resolve) => {
        refreshed = resolve;
      }),
  );
  fireEvent.click(screen.getByRole("button", { name: "刷新拓扑" }));
  view.unmount();
  await act(async () => {
    refreshed(topology);
  });
  expect(screen.queryByText("node-1")).not.toBeInTheDocument();
});
