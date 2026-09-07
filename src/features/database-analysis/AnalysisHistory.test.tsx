import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import AnalysisHistory from "./AnalysisHistory";
import * as api from "./analysisHistoryApi";
import type { DatabaseAnalysisReport } from "../../lib/types";

vi.mock("./analysisHistoryApi", () => ({ listAnalysisHistory: vi.fn(), saveAnalysisHistory: vi.fn(), getAnalysisHistory: vi.fn(), deleteAnalysisHistory: vi.fn() }));
const report: DatabaseAnalysisReport = {
  node_results: [], failed_nodes: [],
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

it("仅显式加载本机历史趋势，按扫描范围隔离图表", async () => {
  vi.mocked(api.listAnalysisHistory).mockResolvedValue([summary, { ...summary, id: "later", saved_at: 2000 }, { ...summary, id: "other", pattern: "other:*" }]);
  vi.mocked(api.getAnalysisHistory).mockImplementation(async ({ id }) => ({ id, connection_id: "one", saved_at: id === "later" ? 2000 : 1000, report: id === "other" ? { ...report, pattern: "other:*" } : id === "later" ? report : savedReport }));
  render(<AnalysisHistory connectionId="one" database={0} report={report} renderReport={renderReport} />);
  await screen.findAllByRole("button", { name: "查看历史报告" });
  expect(api.getAnalysisHistory).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("button", { name: "加载历史趋势" }));
  expect(await screen.findByRole("img", { name: "历史已观察键数趋势" })).toBeInTheDocument();
  expect(screen.getByRole("table", { name: "趋势数据" })).toHaveTextContent("128");
  expect(screen.getByRole("combobox", { name: "趋势扫描范围" })).toBeInTheDocument();
  fireEvent.change(screen.getByRole("combobox", { name: "趋势扫描范围" }), { target: { value: "1" } });
  expect(screen.queryByRole("img", { name: "历史已观察键数趋势" })).not.toBeInTheDocument();
  expect(screen.getByText(/至少需要两份相同扫描范围/)).toBeInTheDocument();
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

const clusterReport = (successful: string[], failed: string[]): DatabaseAnalysisReport => ({
  ...report,
  node_results: successful.map((node_id) => ({ node_id, endpoint: { host: "127.0.0.1", port: 6379 }, report })),
  failed_nodes: failed.map((node_id) => ({ node_id, code: "CONNECTION_FAILED" })),
});

it.each([
  { name: "成功节点集合", current: clusterReport(["a", "b"], ["c"]), previous: clusterReport(["a"], ["c"]) },
  { name: "失败节点集合", current: clusterReport(["a"], ["b"]), previous: clusterReport(["a"], ["c"]) },
])("$name 不同时保留历史详情但不比较差值", async ({ current, previous }) => {
  vi.mocked(api.getAnalysisHistory).mockResolvedValue({ id: "saved", connection_id: "one", saved_at: 1000, report: previous });
  render(<AnalysisHistory connectionId="one" database={0} report={current} renderReport={renderReport} />);
  fireEvent.click(await screen.findByRole("button", { name: "查看历史报告" }));
  expect(await screen.findByText("历史键数 2")).toBeInTheDocument();
  expect(screen.queryByRole("table", { name: "分析结果比较" })).not.toBeInTheDocument();
  expect(screen.getByText(/节点范围不同/)).toBeInTheDocument();
});

it("节点顺序变化仍比较相同节点范围的历史报告", async () => {
  const previous = { ...clusterReport(["b", "a"], ["d", "c"]), total_memory: { ...report.total_memory, total: 64 } };
  vi.mocked(api.getAnalysisHistory).mockResolvedValue({ id: "saved", connection_id: "one", saved_at: 1000, report: previous });
  render(<AnalysisHistory connectionId="one" database={0} report={clusterReport(["a", "b"], ["c", "d"])} renderReport={renderReport} />);
  fireEvent.click(await screen.findByRole("button", { name: "查看历史报告" }));
  expect(await screen.findByRole("table", { name: "分析结果比较" })).toHaveTextContent("+64");
});

it("切换连接丢弃加载中的趋势，部分读取失败只显示有效记录", async () => {
  let resolve!: (value: api.SavedAnalysis) => void;
  vi.mocked(api.getAnalysisHistory).mockReturnValueOnce(new Promise((done) => { resolve = done; }));
  const { rerender } = render(<AnalysisHistory connectionId="one" database={0} report={report} renderReport={renderReport} />);
  await screen.findByRole("button", { name: "查看历史报告" });
  fireEvent.click(screen.getByRole("button", { name: "加载历史趋势" }));
  vi.mocked(api.listAnalysisHistory).mockResolvedValue([]);
  rerender(<AnalysisHistory connectionId="two" database={0} report={null} renderReport={renderReport} />);
  await screen.findByText("当前数据库暂无已保存分析。");
  resolve({ id: "saved", connection_id: "one", saved_at: 1000, report });
  await waitFor(() => expect(screen.queryByRole("region", { name: "本机历史趋势" })).not.toBeInTheDocument());
});

it("历史部分读取失败明确提示并不绘制不足两点的趋势", async () => {
  vi.mocked(api.listAnalysisHistory).mockResolvedValue([summary, { ...summary, id: "broken" }]);
  vi.mocked(api.getAnalysisHistory).mockResolvedValueOnce({ id: "saved", connection_id: "one", saved_at: 1000, report }).mockRejectedValueOnce(new Error("unavailable"));
  render(<AnalysisHistory connectionId="one" database={0} report={report} renderReport={renderReport} />);
  await screen.findAllByRole("button", { name: "查看历史报告" });
  fireEvent.click(screen.getByRole("button", { name: "加载历史趋势" }));
  expect(await screen.findByText(/部分历史报告读取失败/)).toBeInTheDocument();
  expect(screen.queryByRole("img", { name: "历史已观察键数趋势" })).not.toBeInTheDocument();
});
