import { invoke } from "@tauri-apps/api/core";
export interface BulkTask {
  id: string;
  connection_id: string;
  total: number;
  processed: number;
  deleted: number;
  failed: number;
  status: "running" | "completed" | "partial_failure" | "cancelled";
  cancel_requested: boolean;
}
export const listBulkTasks = () => invoke<BulkTask[]>("list_bulk_tasks");
export const cancelBulkTask = (taskId: string) => invoke<void>("cancel_bulk_task", { taskId });
export const startBulkDelete = (connectionId: string, keys: string[]) => invoke<BulkTask>("start_bulk_delete", { input: { connection_id: connectionId, keys } });
export const BULK_TASKS_CHANGED = "redix-bulk-tasks-changed";
export const BULK_TASK_FINISHED = "redix-bulk-task-finished";
