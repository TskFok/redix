import { useEffect, useRef, useState } from "react";
import type { AnalyzeDatabaseInput, DatabaseAnalysisReport } from "../../lib/types";
import { startAnalysisTask, listAnalysisTasks, getAnalysisTask, cancelAnalysisTask, type AnalysisTask } from "./analysisTasksApi";
import { toUserFacingAnalysisError } from "./databaseAnalysisState";

interface State { scope: string; task: AnalysisTask | null; report: DatabaseAnalysisReport | null; pending: boolean; error: string | null }
const taskError = (task: AnalysisTask) => task.status === "failed" ? (task.error_code === "INVALID_ANALYSIS_RESULT" ? "分析结果超出内存上限或范围不符，请缩小扫描范围后重试。" : toUserFacingAnalysisError({ code: task.error_code })) : null;
export function useAnalysisTask(connectionId: string, database: number) {
  const scope = JSON.stringify([connectionId, database]);
  const scopeRef = useRef(scope); scopeRef.current = scope;
  const generation = useRef(0);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const [state, setState] = useState<State>({ scope, task: null, report: null, pending: true, error: null });
  const stateRef = useRef(state); stateRef.current = state;
  const isCurrent = (token: number) => token === generation.current && scopeRef.current === scope;
  const inScope = (task: AnalysisTask) => task.connection_id === connectionId && task.database === database;
  const stopTimer = () => { if (timer.current !== null) clearTimeout(timer.current); timer.current = null; };
  const watch = async (task: AnalysisTask, token: number) => {
    if (!isCurrent(token)) return;
    try {
      const result = await getAnalysisTask({ connection_id: connectionId, database, task_id: task.id });
      if (!isCurrent(token)) return;
      if (!inScope(result.task) || result.task.id !== task.id) {
        setState({ scope, task: null, report: null, pending: false, error: "任务范围不符，未显示此分析结果。" }); return;
      }
      const report = result.report?.database === database ? result.report : null;
      setState({ scope, task: result.task, report, pending: false, error: taskError(result.task) });
      if (result.task.status === "running") timer.current = setTimeout(() => void watch(result.task, token), 500);
    } catch {
      if (!isCurrent(token)) return;
      setState((current) => ({ ...current, scope, task, pending: false, error: "读取后台分析进度失败，将继续重试。" }));
      timer.current = setTimeout(() => void watch(task, token), 1000);
    }
  };
  const recover = async () => {
    const token = ++generation.current;
    stopTimer();
    setState({ scope, task: null, report: null, pending: true, error: null });
    try {
      const tasks = await listAnalysisTasks({ connection_id: connectionId, database });
      if (!isCurrent(token)) return;
      const task = tasks.find(inScope);
      if (task) { setState({ scope, task, report: null, pending: false, error: null }); void watch(task, token); }
      else setState({ scope, task: null, report: null, pending: false, error: null });
    } catch {
      if (isCurrent(token)) setState({ scope, task: null, report: null, pending: false, error: "恢复后台分析失败，请点击重新读取任务。" });
    }
  };
  useEffect(() => {
    void recover();
    return () => { generation.current += 1; stopTimer(); };
    // Task scope owns the polling lifetime; leaving the page does not cancel server work.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [scope]);
  const start = async (analysis: AnalyzeDatabaseInput, timeoutSeconds: number) => {
    if (stateRef.current.scope !== scope || stateRef.current.pending || stateRef.current.task?.status === "running") return;
    const token = ++generation.current;
    stopTimer();
    setState({ scope, task: null, report: null, pending: true, error: null });
    try {
      const task = await startAnalysisTask({ analysis: { ...analysis, connection_id: connectionId }, database, timeout_seconds: timeoutSeconds });
      if (!isCurrent(token)) return;
      if (!inScope(task)) throw new Error("invalid task scope");
      setState({ scope, task, report: null, pending: false, error: null });
      void watch(task, token);
    } catch (error) { if (isCurrent(token)) setState({ scope, task: null, report: null, pending: false, error: toUserFacingAnalysisError(error) }); }
  };
  const cancel = async () => {
    const task = stateRef.current.task;
    const token = generation.current;
    if (!task || !inScope(task) || task.status !== "running") return;
    try {
      await cancelAnalysisTask({ connection_id: connectionId, database, task_id: task.id });
      if (isCurrent(token)) setState((current) => ({ ...current, task: current.task ? { ...current.task, cancel_requested: true } : null }));
    } catch { if (isCurrent(token)) setState((current) => ({ ...current, error: "取消分析失败，请重试；后台进度会继续更新。" })); }
  };
  const current = state.scope === scope ? state : { scope, task: null, report: null, pending: true, error: null };
  return { ...current, loading: current.pending || current.task?.status === "running", start, cancel, recover };
}
