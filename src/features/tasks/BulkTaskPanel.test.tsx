import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import BulkTaskPanel from "./BulkTaskPanel";
import { BULK_TASK_FINISHED, BULK_TASKS_CHANGED, cancelBulkTask, listBulkTasks, type BulkTask } from "./bulkTaskApi";
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
  await waitFor(() => expect(finished).toHaveBeenCalledTimes(1), { timeout: 2000 });
  expect(screen.queryByText(/后台任务（/)).not.toBeInTheDocument();
  expect(screen.queryByRole("alert")).not.toBeInTheDocument();
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
it("任务成功完成后自动关闭面板，新任务启动后重新显示", async () => {
  const running: BulkTask = { id: "task", connection_id: "one", total: 1, processed: 0, deleted: 0, failed: 0, status: "running", cancel_requested: false };
  const completed: BulkTask = { ...running, processed: 1, deleted: 1, status: "completed" };
  vi.mocked(listBulkTasks).mockResolvedValueOnce([running]).mockResolvedValue([completed]);
  render(<BulkTaskPanel />);
  expect(await screen.findByText("运行中")).toBeInTheDocument();
  await waitFor(() => expect(screen.queryByText(/后台任务（/)).not.toBeInTheDocument(), { timeout: 2000 });

  vi.mocked(listBulkTasks).mockResolvedValue([completed, { ...running, id: "next" }]);
  fireEvent(window, new Event(BULK_TASKS_CHANGED));
  expect(await screen.findByText("运行中")).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "取消任务" })).toBeEnabled();
});
it("并发任务全部成功完成后才自动关闭面板", async () => {
  const running: BulkTask = { id: "first", connection_id: "one", total: 1, processed: 0, deleted: 0, failed: 0, status: "running", cancel_requested: false };
  const completed: BulkTask = { ...running, processed: 1, deleted: 1, status: "completed" };
  vi.mocked(listBulkTasks).mockResolvedValue([completed, { ...running, id: "second" }]);
  render(<BulkTaskPanel />);
  expect(await screen.findByText("已完成")).toBeInTheDocument();
  expect(screen.getByText("运行中")).toBeInTheDocument();

  vi.mocked(listBulkTasks).mockResolvedValue([completed, { ...completed, id: "second" }]);
  fireEvent.click(screen.getByRole("button", { name: "刷新任务" }));
  await waitFor(() => expect(screen.queryByText(/后台任务（/)).not.toBeInTheDocument());
});
it("部分失败的任务保留面板以便核对删除结果", async () => {
  vi.mocked(listBulkTasks).mockResolvedValue([{ id: "failed", connection_id: "one", total: 2, processed: 2, deleted: 1, failed: 1, status: "partial_failure", cancel_requested: false }]);
  render(<BulkTaskPanel />);
  expect(await screen.findByText("部分失败")).toBeInTheDocument();
  expect(screen.getByText(/已处理 2 \/ 2 · 已删除 1 · 失败 \/ 结果未知 1/)).toBeInTheDocument();
});
it("手动关闭后继续轮询和通知结果，状态更新不会重新打开面板", async () => {
  const running: BulkTask = { id: "task", connection_id: "one", total: 2, processed: 0, deleted: 0, failed: 0, status: "running", cancel_requested: false };
  const failed: BulkTask = { ...running, processed: 2, deleted: 1, failed: 1, status: "partial_failure" };
  vi.mocked(listBulkTasks).mockResolvedValueOnce([running]).mockResolvedValue([failed]);
  const finished = vi.fn();
  window.addEventListener(BULK_TASK_FINISHED, finished);
  try {
    render(<BulkTaskPanel />);
    fireEvent.click(await screen.findByRole("button", { name: "关闭后台任务" }));
    expect(screen.queryByText(/后台任务（/)).not.toBeInTheDocument();
    expect(cancelBulkTask).not.toHaveBeenCalled();

    await waitFor(() => expect(finished).toHaveBeenCalledTimes(1), { timeout: 2000 });
    expect((finished.mock.calls[0][0] as CustomEvent<BulkTask>).detail).toEqual(failed);
    expect(screen.queryByText(/后台任务（/)).not.toBeInTheDocument();
  } finally {
    window.removeEventListener(BULK_TASK_FINISHED, finished);
  }
});
it("手动关闭结果面板后，新任务启动时重新显示", async () => {
  const cancelled: BulkTask = { id: "old", connection_id: "one", total: 2, processed: 0, deleted: 0, failed: 0, status: "cancelled", cancel_requested: true };
  vi.mocked(listBulkTasks).mockResolvedValue([cancelled]);
  render(<BulkTaskPanel />);
  fireEvent.click(await screen.findByRole("button", { name: "关闭后台任务" }));
  expect(screen.queryByText(/后台任务（/)).not.toBeInTheDocument();

  vi.mocked(listBulkTasks).mockResolvedValue([cancelled, { ...cancelled, id: "new", status: "running", cancel_requested: false }]);
  fireEvent(window, new Event(BULK_TASKS_CHANGED));
  expect(await screen.findByText("运行中")).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "关闭后台任务" })).toBeEnabled();
});
it("后台任务状态读取失败时也可以手动关闭面板", async () => {
  vi.mocked(listBulkTasks).mockRejectedValue(new Error("unavailable"));
  render(<BulkTaskPanel />);
  expect(await screen.findByRole("alert")).toHaveTextContent("后台任务状态读取失败");
  fireEvent.click(screen.getByRole("button", { name: "关闭后台任务" }));
  expect(screen.queryByText(/后台任务（/)).not.toBeInTheDocument();
  expect(screen.queryByRole("alert")).not.toBeInTheDocument();
});
