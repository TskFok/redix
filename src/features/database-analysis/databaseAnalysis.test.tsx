import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import * as api from "./analysisTasksApi";
import { listAnalysisHistory } from "./analysisHistoryApi";
import type { DatabaseAnalysisReport } from "../../lib/types";
import DatabaseAnalysisPage from "./DatabaseAnalysisPage";
vi.mock("./analysisTasksApi", () => ({ startAnalysisTask: vi.fn(), listAnalysisTasks: vi.fn(), getAnalysisTask: vi.fn(), cancelAnalysisTask: vi.fn() }));
vi.mock("./analysisHistoryApi", () => ({ listAnalysisHistory: vi.fn().mockResolvedValue([]), saveAnalysisHistory: vi.fn() }));
const report: DatabaseAnalysisReport = {
  node_results: [], failed_nodes: [],
  database: 0,
  pattern: "*",
  delimiter: ":",
  progress: { scanned: 3, processed: 3, max_keys: 100000, truncated: false },
  total_keys: {
    total: 3,
    observed: 3,
    types: [{ type: "string", total: 2 }, { type: "hash", total: 1 }],
  },
  total_memory: {
    total: 384,
    observed: 2,
    types: [{ type: "string", total: 128 }, { type: "hash", total: 256 }],
  },
  top_keys_by_length: [],
  top_keys_by_memory: [
    { key: "user:2", key_type: "hash", length: 4, memory_bytes: 256, ttl_seconds: 120 },
  ],
  top_namespaces_by_keys: [
    { namespace: "user", keys: 2, memory_bytes: 384, types: [{ type: "string", total: 1 }] },
  ],
  top_namespaces_by_memory: [],
  expiration_groups: [{ label: "No Expiry", keys: 1, memory_bytes: 128 }],
};


const task: api.AnalysisTask = { id: "t1", connection_id: "local", database: 0, analysis: { connection_id: "local", pattern: "*", delimiter: ":", max_keys: 100000 }, status: "completed", progress: report.progress, nodes_total: 1, nodes_completed: 1, error_code: null, started_at: 1, cancel_requested: false };
function resolveReport(value: DatabaseAnalysisReport) {
  vi.mocked(api.startAnalysisTask).mockResolvedValue(task);
  vi.mocked(api.getAnalysisTask).mockResolvedValue({ task, report: value });
}
beforeEach(() => { vi.resetAllMocks(); vi.mocked(listAnalysisHistory).mockResolvedValue([]); vi.mocked(api.listAnalysisTasks).mockResolvedValue([]); resolveReport(report); });
afterEach(cleanup);
async function start() { const button = screen.getByRole("button", { name: "开始分析" }); await waitFor(() => expect(button).toBeEnabled()); fireEvent.click(button); }

it("启动后台任务携带原数据库范围并显示报告与只读建议", async () => {
  const key = { key: "large:profile", key_type: "string", length: 100001, memory_bytes: 100001, ttl_seconds: -1 };
  resolveReport({ ...report, top_keys_by_length: [key], top_keys_by_memory: [key] });
  render(<DatabaseAnalysisPage connectionId="local" activeDatabase={0} />);
  await start();
  expect(await screen.findByRole("region", { name: "本地分析建议" })).toHaveTextContent("100,000");
  expect(screen.getByRole("region", { name: "本地分析建议" })).toHaveTextContent("永不过期");
  expect(screen.getByRole("region", { name: "本地分析建议" })).toHaveTextContent("Top Keys");
  expect(api.startAnalysisTask).toHaveBeenCalledWith({ analysis: task.analysis, database: 0, timeout_seconds: 300 });
});
it("重新进入页面恢复完成报告，不重新扫描", async () => {
  vi.mocked(api.listAnalysisTasks).mockResolvedValue([task]);
  render(<DatabaseAnalysisPage connectionId="local" activeDatabase={0} />);
  expect(await screen.findByText("user:2")).toBeInTheDocument();
  expect(api.startAnalysisTask).not.toHaveBeenCalled();
  expect(screen.getByText(/任务和结果仅保留在本次应用内存/)).toBeInTheDocument();
});
it("旧报告缺少节点字段时安全显示单份结果", async () => {
  const legacy = { ...report };
  delete (legacy as Partial<DatabaseAnalysisReport>).node_results;
  delete (legacy as Partial<DatabaseAnalysisReport>).failed_nodes;
  resolveReport(legacy);
  render(<DatabaseAnalysisPage connectionId="local" activeDatabase={0} />);
  await start();
  expect(await screen.findByText("总键数")).toBeInTheDocument();
  expect(screen.queryByRole("alert")).not.toBeInTheDocument();
});
it("Cluster 部分报告明确失败节点与范围", async () => {
  resolveReport({ ...report, failed_nodes: [{ node_id: "node-b", code: "CONNECTION_FAILED" }], node_results: [{ node_id: "node-a", endpoint: { host: "::1", port: 7000 }, report }] });
  render(<DatabaseAnalysisPage connectionId="local" activeDatabase={0} />);
  await start();
  expect(await screen.findByRole("alert")).toHaveTextContent("1 个主节点分析失败");
  expect(screen.getByText(/node-a.*\[::1\]:7000/)).toBeInTheDocument();
});
it("非法 max_keys 或超时不会启动任务", async () => {
  render(<DatabaseAnalysisPage connectionId="local" activeDatabase={0} />);
  fireEvent.change(screen.getByLabelText("最大扫描键数"), { target: { value: "999" } });
  await start();
  expect(screen.getByRole("alert")).toHaveTextContent("扫描键数必须在 1000 到 1000000 之间");
  expect(api.startAnalysisTask).not.toHaveBeenCalled();
  fireEvent.change(screen.getByLabelText("最大扫描键数"), { target: { value: "1000" } });
  fireEvent.change(screen.getByLabelText("任务超时（秒）"), { target: { value: "901" } });
  await start();
  expect(screen.getByRole("alert")).toHaveTextContent("超时必须在 1 到 900 秒之间");
  expect(api.startAnalysisTask).not.toHaveBeenCalled();
});
it("运行任务展示计数、禁用重复启动并允许取消", async () => {
  const running: api.AnalysisTask = { ...task, status: "running", nodes_completed: 0 };
  vi.mocked(api.listAnalysisTasks).mockResolvedValue([running]);
  vi.mocked(api.getAnalysisTask).mockResolvedValue({ task: running, report: null });
  render(<DatabaseAnalysisPage connectionId="local" activeDatabase={0} />);
  const cancel = await screen.findByRole("button", { name: "取消分析" });
  expect(screen.getByRole("button", { name: "开始分析" })).toBeDisabled();
  expect(screen.getByText(/已扫描 3.*已处理 3/)).toBeInTheDocument();
  fireEvent.click(cancel);
  await waitFor(() => expect(api.cancelAnalysisTask).toHaveBeenCalledWith({ connection_id: "local", database: 0, task_id: "t1" }));
});
it.each([ ["timed_out", "分析已超时"], ["cancelled", "分析已取消"] ] as const)("终态 %s 不展示伪造成功结果", async (status, message) => {
  const terminal = { ...task, status };
  vi.mocked(api.listAnalysisTasks).mockResolvedValue([terminal]);
  vi.mocked(api.getAnalysisTask).mockResolvedValue({ task: terminal, report: null });
  render(<DatabaseAnalysisPage connectionId="local" activeDatabase={0} />);
  expect(await screen.findByText(new RegExp(message))).toBeInTheDocument();
  expect(screen.queryByText("总键数")).not.toBeInTheDocument();
});
it("连接错误只显示固定信息", async () => {
  vi.mocked(api.startAnalysisTask).mockRejectedValue({ code: "CONNECTION_FAILED", message: "/secret/path" });
  render(<DatabaseAnalysisPage connectionId="local" activeDatabase={0} />);
  await start();
  expect(await screen.findByRole("alert")).toHaveTextContent("无法连接 Redis，请检查连接状态");
  expect(screen.queryByText(/secret/)).not.toBeInTheDocument();
});
