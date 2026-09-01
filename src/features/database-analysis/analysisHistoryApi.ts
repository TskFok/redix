import { invoke } from "@tauri-apps/api/core";
import type { DatabaseAnalysisReport } from "../../lib/types";

export interface AnalysisHistoryScope { connection_id: string; database: number }
export interface AnalysisHistorySummary {
  id: string; saved_at: number; database: number; pattern: string;
  total_keys: number; total_memory: number; truncated: boolean;
}
export interface SavedAnalysis {
  id: string; connection_id: string; saved_at: number; report: DatabaseAnalysisReport;
}
export const listAnalysisHistory = (input: AnalysisHistoryScope): Promise<AnalysisHistorySummary[]> => invoke("list_analysis_history", { input });
export const saveAnalysisHistory = (input: { connection_id: string; report: DatabaseAnalysisReport }): Promise<AnalysisHistorySummary> => invoke("save_analysis_history", { input });
export const getAnalysisHistory = (input: AnalysisHistoryScope & { id: string }): Promise<SavedAnalysis> => invoke("get_analysis_history", { input });
export const deleteAnalysisHistory = (input: AnalysisHistoryScope & { id: string }): Promise<void> => invoke("delete_analysis_history", { input });
