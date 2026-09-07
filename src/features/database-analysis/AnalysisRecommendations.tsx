import type { AnalysisKey, DatabaseAnalysisReport } from "../../lib/types";

interface Recommendation {
  id: string;
  key: AnalysisKey;
  title: string;
  evidence: string;
  suggestion: string;
}

const number = (value: number) => value.toLocaleString("zh-CN");
const exceeds = (value: number | null, threshold: number) => value !== null && Number.isFinite(value) && value > threshold;

/** Thresholds mirror RedisInsight's local big-string, big-set, list-compression and shard-hash strategies. */
export function analysisRecommendations(report: DatabaseAnalysisReport): Recommendation[] {
  const keys = new Map<string, AnalysisKey>();
  for (const key of [...report.top_keys_by_memory, ...report.top_keys_by_length]) {
    const id = JSON.stringify([key.key, key.key_type]);
    if (!keys.has(id)) keys.set(id, key);
  }
  const recommendations: Recommendation[] = [];
  for (const [id, key] of keys) {
    const type = key.key_type.toLowerCase();
    if (type === "string" && exceeds(key.memory_bytes, 100_000)) {
      recommendations.push({ id, key, title: "检查大字符串的访问方式", evidence: `内存 ${number(key.memory_bytes!)} B，超过 100,000 B 提示阈值。`, suggestion: "如果经常只更新或读取部分字段，可评估 Hash 或 JSON；若按整体访问，可先测量压缩效果和 CPU 成本。" });
    } else if (type === "set" && exceeds(key.length, 1000)) {
      recommendations.push({ id, key, title: "避免一次读取整个大集合", evidence: `集合有 ${number(key.length!)} 个成员，超过 1,000 提示阈值。`, suggestion: "读取部分数据时可使用 SSCAN 分批遍历；结合实际访问模式评估是否拆分集合。" });
    } else if (type === "list" && exceeds(key.length, 1000)) {
      recommendations.push({ id, key, title: "评估长列表的内存和访问范围", evidence: `列表有 ${number(key.length!)} 个元素，超过 1,000 提示阈值。`, suggestion: "限制单次 LRANGE 的范围；可在独立环境评估列表压缩的内存收益与 CPU 成本。" });
    } else if (type === "hash" && exceeds(key.length, 5000)) {
      recommendations.push({ id, key, title: "评估大 Hash 的拆分边界", evidence: `Hash 有 ${number(key.length!)} 个字段，超过 5,000 提示阈值。`, suggestion: "避免不必要的 HGETALL；按业务实体评估拆分并确认原子性和访问成本，不能仅凭字段数确定收益。" });
    }
  }
  return recommendations;
}

export default function AnalysisRecommendations({ report }: { report: DatabaseAnalysisReport }) {
  const recommendations = analysisRecommendations(report);
  return <section className="database-panel" aria-label="本地分析建议">
    <div className="database-panel-heading"><h3>本地分析建议</h3><span className="panel-hint">只读</span></div>
    <p>仅依据本次报告保留的 Top Keys 生成提示，不代表全库，也不估算未观察键的情况；不自动修改 Redis 数据或配置。</p>
    <p className="browser-helper">已观察 {number(report.total_keys.observed)} 个键，读取内存 {number(report.total_memory.observed)} 个键。{report.progress.truncated ? "扫描已截断，可能遗漏其他大键。" : "扫描期间写入或过期会改变观察结果。"}{(report.failed_nodes?.length ?? 0) > 0 ? "存在失败节点，建议只覆盖成功节点。" : ""}</p>
    {recommendations.length ? <ul className="analysis-recommendations">{recommendations.map((recommendation) => <li key={recommendation.id}>
      <h4>{recommendation.title}</h4><code>{recommendation.key.key}</code>
      <p>{recommendation.evidence}</p><p>{recommendation.suggestion}</p>
      {recommendation.key.ttl_seconds === -1 ? <p className="browser-helper">此键永不过期（TTL = -1）。若它承载临时数据，请核对业务保留期限；持久数据无需因此设置过期时间。</p> : null}
    </li>)}</ul> : <p className="empty-state-compact">保留的 Top Keys 未触发这些规则；这不表示数据库没有其他优化空间。</p>}
  </section>;
}
