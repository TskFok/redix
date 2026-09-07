import { describe, expect, it } from "vitest";
import type { AnalysisKey, DatabaseAnalysisReport } from "../../lib/types";
import { analysisRecommendations } from "./AnalysisRecommendations";
import { analysisScope, groupAnalysisHistory } from "./AnalysisTrends";

const report: DatabaseAnalysisReport = {
  database: 0, pattern: "*", delimiter: ":", progress: { scanned: 10, processed: 10, max_keys: 1000, truncated: false },
  total_keys: { total: 10, observed: 10, types: [] }, total_memory: { total: 100, observed: 8, types: [] },
  top_keys_by_memory: [], top_keys_by_length: [], top_namespaces_by_keys: [], top_namespaces_by_memory: [], expiration_groups: [], node_results: [], failed_nodes: [],
};
const key = (key_type: string, length: number | null, memory_bytes: number | null): AnalysisKey => ({ key: `key:${key_type}`, key_type, length, memory_bytes, ttl_seconds: -1 });

describe("本地分析证据", () => {
  it("规则严格按内存或成员数阈值触发，Top榜重复键只提示一次", () => {
    const keys = [key("string", 10, 100001), key("set", 1001, 1), key("list", 1001, 1), key("hash", 5001, 1)];
    const items = analysisRecommendations({ ...report, top_keys_by_length: keys, top_keys_by_memory: keys });
    expect(items.map((item) => item.key.key_type)).toEqual(["string", "set", "list", "hash"]);
    expect(items[0].evidence).toContain("100,001 B");
    expect(analysisRecommendations({ ...report, top_keys_by_memory: [key("string", 100001, 100000), key("set", 1000, 100001), key("list", 1000, 100001), key("hash", 5000, 100001)] })).toEqual([]);
  });

  it("不可用指标不能视为零或猜测建议，未支持类型不触发", () => {
    expect(analysisRecommendations({ ...report, top_keys_by_memory: [key("string", 999999, null), key("hash", null, 999999), key("json", 999999, 999999), key("string", 1, NaN)] })).toEqual([]);
  });

  it("历史按扫描参数和成功失败节点范围隔离，组内按保存时间排序", () => {
    const saved = (id: string, saved_at: number, value = report) => ({ id, saved_at, connection_id: "local", report: value });
    const groups = groupAnalysisHistory([saved("later", 2000), saved("first", 1000), saved("other-limit", 3000, { ...report, progress: { ...report.progress, max_keys: 2000 } }), saved("partial", 4000, { ...report, failed_nodes: [{ node_id: "b", code: "CONNECTION_FAILED" }] })]);
    expect(groups).toHaveLength(3);
    expect(groups[0].reports.map((item) => item.id)).toEqual(["first", "later"]);
    expect(analysisScope(report)).not.toEqual(analysisScope({ ...report, delimiter: "/" }));
    expect(analysisScope(report)).not.toEqual(analysisScope({ ...report, pattern: "user:*" }));
  });

  it("旧报告节点字段缺失仍与单节点范围兼容，非法时间不进入图表", () => {
    const legacy = { ...report };
    delete (legacy as Partial<DatabaseAnalysisReport>).node_results;
    delete (legacy as Partial<DatabaseAnalysisReport>).failed_nodes;
    expect(analysisScope(legacy)).toBe(analysisScope(report));
    expect(groupAnalysisHistory([{ id: "bad", saved_at: NaN, connection_id: "local", report }])).toEqual([]);
  });
});
