import { invoke } from "@tauri-apps/api/core";
import type { AnalyzeDatabaseInput, DatabaseAnalysisReport } from "../../lib/types";

export interface AnalysisTask {
  id: string;
  connection_id: string;
  database: number;
  analysis: AnalyzeDatabaseInput;
  status: "running" | "completed" | "partial_failure" | "cancelled" | "timed_out" | "failed";
  progress: DatabaseAnalysisReport["progress"];
  nodes_total: number;
  nodes_completed: number;
  error_code: string | null;
  started_at: number;
  cancel_requested: boolean;
}
export interface AnalysisTaskScope { connection_id: string; database: number }
export interface AnalysisTaskKey extends AnalysisTaskScope { task_id: string }
export interface AnalysisTaskResult { task: AnalysisTask; report: DatabaseAnalysisReport | null }
export const startAnalysisTask = (input: { analysis: AnalyzeDatabaseInput; database: number; timeout_seconds: number }) => invoke<AnalysisTask>("start_analysis_task", { input });
export const listAnalysisTasks = (input: AnalysisTaskScope) => invoke<AnalysisTask[]>("list_analysis_tasks", { input });
export const getAnalysisTask = (input: AnalysisTaskKey) => invoke<AnalysisTaskResult>("get_analysis_task", { input });
export const cancelAnalysisTask = (input: AnalysisTaskKey) => invoke<void>("cancel_analysis_task", { input });
