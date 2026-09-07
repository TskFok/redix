import { useEffect, useRef, useState } from "react";
import { BULK_TASK_FINISHED, BULK_TASKS_CHANGED, cancelBulkTask, listBulkTasks, type BulkTask } from "./bulkTaskApi";
const labels: Record<BulkTask["status"], string> = { running: "运行中", completed: "已完成", partial_failure: "部分失败", cancelled: "已取消" };
export default function BulkTaskPanel() {
  const [tasks, setTasks] = useState<BulkTask[]>([]);
  const [error, setError] = useState("");
  const refreshRef = useRef<() => void>(() => {});
  useEffect(() => {
    let disposed = false;
    let loading = false;
    let rerun = false;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const seenCompleted = new Set<string>();
    let failures = 0;
    const refresh = async () => {
      if (disposed) return;
      if (loading) { rerun = true; return; }
      clearTimeout(timer);
      loading = true;
      let running = false;
      let retry = false;
      try {
        const result = await listBulkTasks();
        if (disposed) return;
        const next = Array.isArray(result) ? result : [];
        const retainedIds = new Set(next.map((task) => task.id));
        for (const id of seenCompleted) if (!retainedIds.has(id)) seenCompleted.delete(id);
        for (const task of next) {
          if (task.status === "running") { running = true; }
          else if (!seenCompleted.has(task.id)) {
            seenCompleted.add(task.id);
            window.dispatchEvent(new CustomEvent(BULK_TASK_FINISHED, { detail: task }));
          }
        }
        failures = 0; setTasks(next); setError("");
      } catch { retry = ++failures <= 5; if (!disposed) setError("后台任务状态读取失败，正在重试；也可手动刷新。"); }
      finally {
        loading = false;
        if (!disposed && (running || rerun || retry)) { rerun = false; timer = setTimeout(() => void refresh(), 800); }
      }
    };
    refreshRef.current = () => void refresh();
    const listener = () => { failures = 0; void refresh(); };
    window.addEventListener(BULK_TASKS_CHANGED, listener);
    void refresh();
    return () => { disposed = true; clearTimeout(timer); window.removeEventListener(BULK_TASKS_CHANGED, listener); };
  }, []);
  const cancel = async (id: string) => {
    try { await cancelBulkTask(id); refreshRef.current(); }
    catch { setError("取消任务失败，请刷新后重试。"); }
  };
  if (!tasks.length && !error) return null;
  return <details className="database-panel bulk-task-panel" open>
    <summary>后台任务（{tasks.length}）</summary>
    <p className="browser-helper">任务只保留在本次应用会话。取消会等待已发送的命令返回，已删除的键不会恢复；失败项可能已生效，请核对后再操作。</p>
    <button className="button button-secondary" onClick={() => refreshRef.current()}>刷新任务</button>
    {error && <p role="alert">{error}</p>}
    {tasks.slice().reverse().map((task) => <article key={task.id} aria-label={`批量删除任务 ${task.id}`}>
      <strong>{labels[task.status]}</strong> <span>连接：{task.connection_id}</span>
      <p>已处理 {task.processed} / {task.total} · 已删除 {task.deleted} · 失败 / 结果未知 {task.failed}</p>
      <progress value={task.processed} max={task.total} aria-label="删除进度" />
      {task.status === "running" && <button className="button button-secondary" disabled={task.cancel_requested} onClick={() => void cancel(task.id)}>{task.cancel_requested ? "正在取消…" : "取消任务"}</button>}
    </article>)}
  </details>;
}
