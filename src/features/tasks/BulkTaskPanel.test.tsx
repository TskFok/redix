import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import BulkTaskPanel from "./BulkTaskPanel";
import { cancelBulkTask, listBulkTasks } from "./bulkTaskApi";
vi.mock("./bulkTaskApi", async (original) => ({ ...await original<object>(), listBulkTasks: vi.fn(), cancelBulkTask: vi.fn() }));
afterEach(() => { cleanup(); vi.clearAllMocks(); });
it("恢复后台任务进度并允许取消，展示部分失败而不宣称全量删除", async () => {
  vi.mocked(listBulkTasks).mockResolvedValue([{ id: "task", connection_id: "one", total: 10, processed: 3, deleted: 2, failed: 1, status: "running", cancel_requested: false }]);
  vi.mocked(cancelBulkTask).mockResolvedValue();
  render(<BulkTaskPanel />);
  expect(await screen.findByText(/已处理 3 \/ 10/)).toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "取消任务" }));
  await waitFor(() => expect(cancelBulkTask).toHaveBeenCalledWith("task"));
  expect(screen.getByText(/结果未知 1/)).toBeInTheDocument();
});
it("任务中心能展示停止后的真实进度", async () => {
  vi.mocked(listBulkTasks).mockResolvedValue([{ id: "task", connection_id: "one", total: 10, processed: 3, deleted: 3, failed: 0, status: "cancelled", cancel_requested: true }]);
  render(<BulkTaskPanel />);
  expect(await screen.findByText("已取消")).toBeInTheDocument();
  expect(screen.getByText(/已处理 3 \/ 10/)).toBeInTheDocument();
  expect(screen.queryByRole("button", { name: "取消任务" })).not.toBeInTheDocument();
});
it("轮询失败会继续读取，第一次就完成的任务也发出完成通知", async () => {
  const finished = vi.fn();
  window.addEventListener("redix-bulk-task-finished", finished);
  const task = { id: "fast", connection_id: "one", total: 1, processed: 1, deleted: 1, failed: 0, status: "completed" as const, cancel_requested: false };
  vi.mocked(listBulkTasks).mockRejectedValueOnce(new Error("transient")).mockResolvedValue([task]);
  render(<BulkTaskPanel />);
  await screen.findByText("已完成", {}, { timeout: 2000 });
  expect(finished).toHaveBeenCalledTimes(1);
  window.removeEventListener("redix-bulk-task-finished", finished);
});
it("启动任务通知能重启停止的轮询", async () => {
  const api = await import("./bulkTaskApi");
  vi.mocked(listBulkTasks).mockResolvedValue([]);
  render(<BulkTaskPanel />);
  await waitFor(() => expect(listBulkTasks).toHaveBeenCalledOnce());
  vi.mocked(listBulkTasks).mockResolvedValue([{ id: "new", connection_id: "one", total: 1, processed: 0, deleted: 0, failed: 0, status: "running", cancel_requested: false }]);
  fireEvent(window, new Event(api.BULK_TASKS_CHANGED));
  expect(await screen.findByText("运行中")).toBeInTheDocument();
});
