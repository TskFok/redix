import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import AnalysisHistory from "./AnalysisHistory";
import * as api from "./analysisHistoryApi";
import type { DatabaseAnalysisReport } from "../../lib/types";

vi.mock("./analysisHistoryApi", () => ({ listAnalysisHistory: vi.fn(), saveAnalysisHistory: vi.fn(), getAnalysisHistory: vi.fn(), deleteAnalysisHistory: vi.fn() }));
const report: DatabaseAnalysisReport = {
  database: 0, pattern: "*", delimiter: ":", progress: { scanned: 2, processed: 2, max_keys: 1000, truncated: false },
  total_keys: { total: 2, observed: 2, types: [] }, total_memory: { total: 128, observed: 2, types: [] },
  top_keys_by_length: [], top_keys_by_memory: [], top_namespaces_by_keys: [], top_namespaces_by_memory: [], expiration_groups: [],
};
const summary = { id: "saved", saved_at: 1000, database: 0, pattern: "*", total_keys: 1, total_memory: 64, truncated: false };
const savedReport = { ...report, total_keys: { ...report.total_keys, total: 1 }, total_memory: { ...report.total_memory, total: 64 } };
beforeEach(() => {
  vi.resetAllMocks();
  vi.mocked(api.listAnalysisHistory).mockResolvedValue([summary]);
  vi.mocked(api.getAnalysisHistory).mockResolvedValue({ id: "saved", connection_id: "one", saved_at: 1000, report: savedReport });
  vi.mocked(api.saveAnalysisHistory).mockResolvedValue(summary);
  vi.mocked(api.deleteAnalysisHistory).mockResolvedValue(undefined);
});
afterEach(cleanup);
const renderReport = (value: DatabaseAnalysisReport) => <p>历史键数 {value.total_keys.total}</p>;

it("只在显式点击后保存，并展示键名隐私提示", async () => {
  render(<AnalysisHistory connectionId="one" database={0} report={report} renderReport={renderReport} />);
  expect(screen.getByText(/报告可能包含键名/)).toBeInTheDocument();
  await screen.findByRole("button", { name: "查看历史报告" });
  expect(api.saveAnalysisHistory).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("button", { name: "保存当前分析" }));
  await screen.findByText("分析报告已保存到本机。");
  expect(api.saveAnalysisHistory).toHaveBeenCalledWith({ connection_id: "one", report });
});

it("查看完整历史并比较同参数当前结果，删除必须确认", async () => {
  render(<AnalysisHistory connectionId="one" database={0} report={report} renderReport={renderReport} />);
  fireEvent.click(await screen.findByRole("button", { name: "查看历史报告" }));
  expect(await screen.findByText("历史键数 1")).toBeInTheDocument();
  expect(screen.getByRole("table", { name: "分析结果比较" })).toHaveTextContent("+64");
  fireEvent.click(screen.getByRole("button", { name: "删除历史报告" }));
  expect(api.deleteAnalysisHistory).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("button", { name: "确认删除报告" }));
  await waitFor(() => expect(screen.queryByText("历史键数 1")).not.toBeInTheDocument());
  expect(api.deleteAnalysisHistory).toHaveBeenCalledWith({ connection_id: "one", database: 0, id: "saved" });
});

it("数据库切换后丢弃旧详情响应", async () => {
  let resolve!: (value: api.SavedAnalysis) => void;
  vi.mocked(api.getAnalysisHistory).mockReturnValueOnce(new Promise((r) => { resolve = r; }));
  const { rerender } = render(<AnalysisHistory connectionId="one" database={0} report={report} renderReport={renderReport} />);
  fireEvent.click(await screen.findByRole("button", { name: "查看历史报告" }));
  vi.mocked(api.listAnalysisHistory).mockResolvedValue([]);
  rerender(<AnalysisHistory connectionId="one" database={1} report={null} renderReport={renderReport} />);
  await screen.findByText("当前数据库暂无已保存分析。");
  resolve({ id: "saved", connection_id: "one", saved_at: 1000, report: savedReport });
  await waitFor(() => expect(screen.queryByText("历史键数 1")).not.toBeInTheDocument());
});

it("不同扫描参数不展示误导性差值，失败文案不暴露底层路径", async () => {
  render(<AnalysisHistory connectionId="one" database={0} report={{ ...report, pattern: "user:*" }} renderReport={renderReport} />);
  fireEvent.click(await screen.findByRole("button", { name: "查看历史报告" }));
  expect(await screen.findByText(/扫描参数不同/)).toBeInTheDocument();
  expect(screen.queryByRole("table", { name: "分析结果比较" })).not.toBeInTheDocument();
  vi.mocked(api.saveAnalysisHistory).mockRejectedValue(new Error("/private/secret"));
  fireEvent.click(screen.getByRole("button", { name: "保存当前分析" }));
  expect(await screen.findByRole("alert")).toHaveTextContent("历史记录操作失败");
  expect(screen.queryByText(/private/)).not.toBeInTheDocument();
});
