export interface PlotPoint { x: number; y: number }
export interface PlotSeries { name: string; points: PlotPoint[] }
export type BuiltinVisualizationData =
  | { kind: "timeseries"; series: PlotSeries[]; total: number }
  | { kind: "geo"; points: (PlotPoint & { name: string })[]; missing: string[] }
  | { error: string };

const MAX_POINTS = 2000;
const MAX_SERIES = 20;
const displayName = (value: string) => value.length > 256 ? `${value.slice(0, 256)}…` : value;

// Match the backend's quote/escape rules. This only classifies a completed result;
// it never rewrites or executes commands.
function commandTokens(command: string): string[] | null {
  if (command.length > 32768) return null;
  const tokens: string[] = [];
  let token = "", quote = "", escaped = false, started = false;
  for (const char of command) {
    if (escaped) { token += char; escaped = false; started = true; continue; }
    if (char === "\\") { escaped = true; started = true; continue; }
    if (quote) { if (char === quote) quote = ""; else token += char; continue; }
    if (char === '"' || char === "'") { quote = char; started = true; continue; }
    if (/\s/u.test(char)) {
      if (started) { tokens.push(token); token = ""; started = false; }
    } else { token += char; started = true; }
  }
  if (quote || escaped) return null;
  if (started) tokens.push(token);
  return tokens;
}

function finiteNumber(value: unknown): number {
  if (typeof value !== "number" && (typeof value !== "string" || value.length > 256 || !/^[+-]?(?:\d+(?:\.\d*)?|\.\d+)(?:e[+-]?\d+)?$/i.test(value))) {
    throw new Error("结果包含无法绘制的数值，请查看原始结果。");
  }
  const number = Number(value);
  if (!Number.isFinite(number)) throw new Error("结果包含非有限数值，请查看原始结果。");
  return number;
}

function array(value: unknown): unknown[] {
  if (!Array.isArray(value)) throw new Error("结果结构不符合该命令的可视化格式，请查看原始结果。");
  return value;
}

function samples(value: unknown): PlotPoint[] {
  const rows = array(value);
  if (rows.length > MAX_POINTS) throw new Error(`图表最多显示 ${MAX_POINTS} 个样本，请在命令中增加 COUNT 或缩小范围。`);
  return rows.map((row) => {
    const pair = array(row);
    if (pair.length !== 2) throw new Error("时间序列样本必须包含时间戳和值。");
    const x = finiteNumber(pair[0]), y = finiteNumber(pair[1]);
    if (!Number.isSafeInteger(x) || x < 0 || x > 8640000000000000) throw new Error("时间戳超出可绘制范围。");
    return { x, y };
  }).sort((a, b) => a.x - b.x);
}

function coordinate(value: unknown): PlotPoint {
  const pair = array(value);
  if (pair.length !== 2) throw new Error("地理结果必须包含经度和纬度。");
  const x = finiteNumber(pair[0]), y = finiteNumber(pair[1]);
  if (Math.abs(x) > 180 || Math.abs(y) > 90) throw new Error("经纬度超出有效范围。");
  return { x, y };
}

function geoOptions(tokens: string[]): string[] {
  const name = tokens[0].toUpperCase();
  if (name === "GEORADIUS" || name === "GEORADIUS_RO") return tokens.slice(6);
  if (name === "GEORADIUSBYMEMBER" || name === "GEORADIUSBYMEMBER_RO") return tokens.slice(5);
  let offset = 2;
  if (tokens[offset]?.toUpperCase() === "FROMMEMBER") offset += 2;
  else if (tokens[offset]?.toUpperCase() === "FROMLONLAT") offset += 3;
  else throw new Error("无法识别 GEOSEARCH 查询中心。");
  if (tokens[offset]?.toUpperCase() === "BYRADIUS") offset += 3;
  else if (tokens[offset]?.toUpperCase() === "BYBOX") offset += 4;
  else throw new Error("无法识别 GEOSEARCH 查询范围。");
  return tokens.slice(offset);
}

export function parseVisualization(command: string, value: unknown): BuiltinVisualizationData | null {
  const tokens = commandTokens(command);
  const name = tokens?.[0]?.toUpperCase();
  if (!tokens || !name) return null;
  const single = ["TS.RANGE", "TS.REVRANGE", "TS.GET"].includes(name);
  const multi = ["TS.MRANGE", "TS.MREVRANGE", "TS.MGET"].includes(name);
  const geo = ["GEOPOS", "GEOSEARCH", "GEORADIUS", "GEORADIUS_RO", "GEORADIUSBYMEMBER", "GEORADIUSBYMEMBER_RO"].includes(name);
  if (!single && !multi && !geo) return null;
  try {
    if (single || multi) {
      let series: PlotSeries[];
      if (single) {
        const result = name === "TS.GET" ? (value !== null && array(value).length ? [value] : []) : value;
        series = [{ name: displayName(tokens[1] ?? "时间序列"), points: samples(result) }];
      } else {
        const rows = array(value);
        if (rows.length > MAX_SERIES) throw new Error(`图表最多显示 ${MAX_SERIES} 条序列，请缩小 FILTER 范围。`);
        let count = 0;
        series = rows.map((row) => {
          const entry = array(row);
          if (entry.length !== 3 || typeof entry[0] !== "string" || !Array.isArray(entry[1])) throw new Error("多序列结果结构不受支持，请查看原始结果。");
          const points = samples(name === "TS.MGET" && entry[2] !== null && array(entry[2]).length ? [entry[2]] : entry[2] ?? []);
          count += points.length;
          if (count > MAX_POINTS) throw new Error(`图表最多显示 ${MAX_POINTS} 个样本，请缩小查询范围。`);
          return { name: displayName(entry[0]), points };
        });
      }
      return { kind: "timeseries", series, total: series.reduce((sum, item) => sum + item.points.length, 0) };
    }
    const rows = array(value);
    if (rows.length > MAX_POINTS) throw new Error(`图表最多显示 ${MAX_POINTS} 个位置，请缩小查询范围。`);
    const points: (PlotPoint & { name: string })[] = [], missing: string[] = [];
    if (name === "GEOPOS") {
      if (rows.length !== tokens.length - 2) throw new Error("坐标数量与请求成员不一致。");
      rows.forEach((row, index) => {
        if (row === null) missing.push(displayName(tokens[index + 2]));
        else points.push({ name: displayName(tokens[index + 2]), ...coordinate(row) });
      });
    } else {
      const options = geoOptions(tokens).map((item) => item.toUpperCase());
      if (!options.includes("WITHCOORD")) throw new Error("地理图表需要 WITHCOORD；请修改命令后重新执行。");
      const coordinateIndex = 1 + Number(options.includes("WITHDIST")) + Number(options.includes("WITHHASH"));
      rows.forEach((row) => {
        const entry = array(row);
        if (entry.length !== coordinateIndex + 1 || typeof entry[0] !== "string") throw new Error("地理结果结构与查询选项不一致。");
        points.push({ name: displayName(entry[0]), ...coordinate(entry[coordinateIndex]) });
      });
    }
    return { kind: "geo", points, missing };
  } catch (error) {
    return { error: error instanceof Error ? error.message : "结果无法可视化，请查看原始结果。" };
  }
}
