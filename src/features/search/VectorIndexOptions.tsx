import Select from "../../components/Select";
import type { SearchVectorConfig } from "../../lib/types";

export const defaultSearchVectorConfig = (): SearchVectorConfig => ({ algorithm: "FLAT", data_type: "FLOAT32", dimension: 384, distance_metric: "COSINE" });

export function validSearchVectorConfig(config: SearchVectorConfig | null | undefined): boolean {
  if (!config) return false;
  const positive = (value: number, max: number) => Number.isInteger(value) && value > 0 && value <= max;
  if (!positive(config.dimension, 32768) || !["FLAT", "HNSW"].includes(config.algorithm)
    || !["FLOAT32", "FLOAT64"].includes(config.data_type) || !["L2", "IP", "COSINE"].includes(config.distance_metric)) return false;
  const limits = { initial_capacity: 1_000_000, block_size: 1_000_000, m: 512, ef_construction: 4096, ef_runtime: 4096 };
  for (const key of Object.keys(limits) as Array<keyof typeof limits>) {
    const value = config[key];
    if (value !== undefined && !positive(value, limits[key])) return false;
  }
  return config.algorithm === "FLAT"
    ? config.m === undefined && config.ef_construction === undefined && config.ef_runtime === undefined
    : config.block_size === undefined;
}

export function VectorIndexOptions({ value, index, disabled, onChange }: {
  value: SearchVectorConfig;
  index: number;
  disabled: boolean;
  onChange: (value: SearchVectorConfig) => void;
}) {
  const label = `字段 ${index + 1}`;
  const optionalFields = value.algorithm === "HNSW"
    ? [{ key: "m", label: "M", max: 512 }, { key: "ef_construction", label: "EF_CONSTRUCTION", max: 4096 }, { key: "ef_runtime", label: "EF_RUNTIME", max: 4096 }]
    : [{ key: "block_size", label: "BLOCK_SIZE", max: 1_000_000 }];
  return <fieldset className="editor-fieldset search-vector-options" disabled={disabled}>
    <legend>{label} VECTOR 配置</legend>
    <div className="form-grid">
      <label className="field"><span>算法</span><Select aria-label={`${label} 向量算法`} value={value.algorithm} onChange={(event) => {
        const { block_size: _block, m: _m, ef_construction: _construction, ef_runtime: _runtime, ...common } = value;
        onChange({ ...common, algorithm: event.target.value as SearchVectorConfig["algorithm"] });
      }}>{["FLAT", "HNSW"].map((algorithm) => <option key={algorithm}>{algorithm}</option>)}</Select></label>
      <label className="field"><span>数据类型</span><Select aria-label={`${label} 向量数据类型`} value={value.data_type} onChange={(event) => onChange({ ...value, data_type: event.target.value as SearchVectorConfig["data_type"] })}>{["FLOAT32", "FLOAT64"].map((type) => <option key={type}>{type}</option>)}</Select></label>
      <label className="field"><span>维度（1–32768）</span><input autoCapitalize="off" autoCorrect="off" aria-label={`${label} 向量维度`} type="number" min="1" max="32768" step="1" value={value.dimension || ""} onChange={(event) => onChange({ ...value, dimension: Number(event.target.value) })} /></label>
      <label className="field"><span>距离度量</span><Select aria-label={`${label} 向量距离度量`} value={value.distance_metric} onChange={(event) => onChange({ ...value, distance_metric: event.target.value as SearchVectorConfig["distance_metric"] })}>{["COSINE", "L2", "IP"].map((metric) => <option key={metric}>{metric}</option>)}</Select></label>
      {[{ key: "initial_capacity", label: "INITIAL_CAP", max: 1_000_000 }, ...optionalFields].map((option) => <label className="field" key={option.key}><span>{option.label}（可选）</span><input autoCapitalize="off" autoCorrect="off" aria-label={`${label} ${option.label}`} type="number" min="1" max={option.max} step="1" value={value[option.key as keyof SearchVectorConfig] ?? ""} onChange={(event) => onChange({ ...value, [option.key]: event.target.value === "" ? undefined : Number(event.target.value) })} /></label>)}
    </div>
    <small>可选项留空使用 Redis 默认值；切换算法会清除不适用的选项。</small>
  </fieldset>;
}
