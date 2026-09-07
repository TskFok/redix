import type { SearchIndexAttribute } from "../../lib/types";

export type SearchFilterType = "TEXT" | "TAG" | "NUMERIC" | "GEO";
export interface SearchFilter {
  field: string;
  type: SearchFilterType;
  value: string;
  upper?: string;
  latitude?: string;
  radius?: string;
  unit?: string;
}

const safeField = /^[A-Za-z_][A-Za-z0-9_]*$/;
const supportedTypes = new Set(["TEXT", "TAG", "NUMERIC", "GEO"]);

export function queryBuilderFields(attributes: SearchIndexAttribute[]) {
  return attributes.flatMap((attribute) => {
    const name = attribute.query_name ?? attribute.identifier;
    const type = attribute.field_type.toUpperCase();
    return !attribute.no_index && safeField.test(name) && supportedTypes.has(type)
      ? [{ name, type: type as SearchFilterType }]
      : [];
  });
}

function numberValue(value: string | undefined): number | null {
  if (value === undefined || !/^[+-]?(?:\d+(?:\.\d*)?|\.\d+)(?:[eE][+-]?\d+)?$/.test(value.trim())) return null;
  const parsed = Number(value);
  return Number.isFinite(parsed) ? parsed : null;
}

function escapeLiteral(value: string, escapeSpaces: boolean) {
  return [...value].map((character) => {
    if (/[\p{L}\p{N}_]/u.test(character) || (!escapeSpaces && character === " ")) return character;
    return `\\${character}`;
  }).join("");
}

export function buildSearchQuery(filters: SearchFilter[]): { query: string | null; error: string | null } {
  const invalid = (message: string) => ({ query: null, error: message });
  if (filters.length > 8) return invalid("最多添加 8 个条件。");
  const clauses: string[] = [];
  for (const filter of filters) {
    if (!safeField.test(filter.field)) return invalid("字段名称暂不支持可视构建，请直接编辑查询语句。");
    const value = filter.value.trim();
    const prefix = `@${filter.field}:`;
    switch (filter.type) {
      case "TEXT":
      case "TAG":
        if (!value || /[\u0000-\u001f\u007f]/.test(value)) return invalid("文本或标签条件不能为空或包含控制字符。");
        clauses.push(filter.type === "TEXT"
          ? `${prefix}("${escapeLiteral(value, false)}")`
          : `${prefix}{${escapeLiteral(value, true)}}`);
        break;
      case "NUMERIC": {
        const lower = value === "" || value === "-inf" ? -Infinity : numberValue(value);
        const upper = !filter.upper?.trim() || filter.upper.trim() === "+inf" ? Infinity : numberValue(filter.upper);
        if (lower === null || upper === null || lower > upper) return invalid("数值范围无效，下限不能大于上限。");
        clauses.push(`${prefix}[${lower === -Infinity ? "-inf" : lower} ${upper === Infinity ? "+inf" : upper}]`);
        break;
      }
      case "GEO": {
        const longitude = numberValue(value);
        const latitude = numberValue(filter.latitude);
        const radius = numberValue(filter.radius);
        if (longitude === null || Math.abs(longitude) > 180 || latitude === null || Math.abs(latitude) > 85.05112878 || radius === null || radius <= 0 || !["m", "km", "mi", "ft"].includes(filter.unit ?? "")) {
          return invalid("请填写有效的经纬度、正数半径和距离单位。");
        }
        clauses.push(`${prefix}[${longitude} ${latitude} ${radius} ${filter.unit}]`);
        break;
      }
      default:
        return invalid("此字段类型暂不支持可视构建。");
    }
  }
  const query = clauses.join(" ") || "*";
  return new TextEncoder().encode(query).length > 4096
    ? invalid("生成的查询超过 4096 字节，请减少条件内容。")
    : { query, error: null };
}
