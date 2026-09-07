import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { useAnalysisTask } from "./useAnalysisTask";
import * as api from "./analysisTasksApi";
import type { DatabaseAnalysisReport } from "../../lib/types";

vi.mock("./analysisTasksApi", () => ({ startAnalysisTask: vi.fn(), listAnalysisTasks: vi.fn(), getAnalysisTask: vi.fn(), cancelAnalysisTask: vi.fn() }));
const analysis = { connection_id: "one", pattern: "*", delimiter: ":", max_keys: 1000 };
const task = { id: "t1", connection_id: "one", database: 0, analysis, status: "running" as const, progress: { scanned: 100, processed: 80, max_keys: 1000, truncated: false }, nodes_total: 1, nodes_completed: 0, error_code: null, started_at: 1, cancel_requested: false };
const report: DatabaseAnalysisReport = { database: 0, pattern: "*", delimiter: ":", progress: { ...task.progress }, total_keys: { total: 80, observed: 80, types: [] }, total_memory: { total: 0, observed: 0, types: [] }, top_keys_by_length: [], top_keys_by_memory: [], top_namespaces_by_keys: [], top_namespaces_by_memory: [], expiration_groups: [], node_results: [], failed_nodes: [] };
function Harness({ database = 0 }: { database?: number }) {
  const job = useAnalysisTask("one", database);
  return <><button disabled={job.loading} onClick={() => void job.start(analysis, 60)}>开始</button><button onClick={() => void job.cancel()}>取消</button><span>{job.task?.status ?? "empty"}</span>{job.report && <span>结果 {job.report.database}</span>}{job.error && <span role="alert">{job.error}</span>}</>;
}
beforeEach(() => { vi.resetAllMocks(); vi.mocked(api.listAnalysisTasks).mockResolvedValue([]); vi.mocked(api.startAnalysisTask).mockResolvedValue(task); vi.mocked(api.getAnalysisTask).mockResolvedValue({ task, report: null }); vi.mocked(api.cancelAnalysisTask).mockResolvedValue(undefined); });
afterEach(cleanup);

it("切页返回恢复运行任务，取消使用任务原数据库范围", async () => {
  vi.mocked(api.listAnalysisTasks).mockResolvedValue([task]);
  const first = render(<Harness />);
  expect(await screen.findByText("running")).toBeInTheDocument();
  first.unmount();
  expect(api.cancelAnalysisTask).not.toHaveBeenCalled();
  render(<Harness />);
  expect(await screen.findByText("running")).toBeInTheDocument();
  fireEvent.click(screen.getByText("取消"));
  await waitFor(() => expect(api.cancelAnalysisTask).toHaveBeenCalledWith({ connection_id: "one", database: 0, task_id: "t1" }));
});

it("旧数据库的延迟结果不会进入新数据库", async () => {
  let resolve!: (result: api.AnalysisTaskResult) => void;
  vi.mocked(api.listAnalysisTasks).mockResolvedValueOnce([task]).mockResolvedValue([]);
  vi.mocked(api.getAnalysisTask).mockReturnValueOnce(new Promise((done) => { resolve = done; }));
  const view = render(<Harness />);
  await waitFor(() => expect(api.getAnalysisTask).toHaveBeenCalled());
  view.rerender(<Harness database={1} />);
  await waitFor(() => expect(screen.getByText("开始")).toBeEnabled());
  resolve({ task: { ...task, status: "completed" }, report });
  await waitFor(() => expect(screen.queryByText("结果 0")).not.toBeInTheDocument());
});

it("轮询临时失败后仍继续读取并恢复完成结果", async () => {
  vi.mocked(api.listAnalysisTasks).mockResolvedValue([task]);
  vi.mocked(api.getAnalysisTask).mockRejectedValueOnce(new Error("temporary")).mockResolvedValue({ task: { ...task, status: "completed" }, report });
  render(<Harness />);
  expect(await screen.findByRole("alert")).toHaveTextContent("继续重试");
  expect(await screen.findByText("结果 0", {}, { timeout: 2500 })).toBeInTheDocument();
  expect(screen.queryByRole("alert")).not.toBeInTheDocument();
});

it("启动任务携带数据库及超时且只接收同范围报告", async () => {
  vi.mocked(api.getAnalysisTask).mockResolvedValue({ task: { ...task, status: "completed" }, report: { ...report, database: 1 } });
  render(<Harness />);
  await waitFor(() => expect(screen.getByText("开始")).toBeEnabled());
  fireEvent.click(screen.getByText("开始"));
  await waitFor(() => expect(api.startAnalysisTask).toHaveBeenCalledWith({ analysis, database: 0, timeout_seconds: 60 }));
  await waitFor(() => expect(screen.getByText("开始")).toBeEnabled());
  expect(screen.queryByText("结果 1")).not.toBeInTheDocument();
});
