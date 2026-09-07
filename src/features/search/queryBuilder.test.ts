import { describe, expect, it } from "vitest";
import { buildSearchQuery, queryBuilderFields, type SearchFilter } from "./queryBuilder";

describe("可视查询构建器", () => {
  it("按 AND 合并文本短语、标签、数值范围与地理范围并转义查询操作符", () => {
    const filters: SearchFilter[] = [
      { field: "name", type: "TEXT", value: 'Alice" | *' },
      { field: "tags", type: "TAG", value: "red|blue team" },
      { field: "price", type: "NUMERIC", value: "10", upper: "20" },
      { field: "location", type: "GEO", value: "120", latitude: "30", radius: "5", unit: "km" },
    ];
    expect(buildSearchQuery(filters)).toEqual({ query: '@name:("Alice\\" \\| \\*") @tags:{red\\|blue\\ team} @price:[10 20] @location:[120 30 5 km]', error: null });
  });

  it.each([
    { field: "name|secret", type: "TEXT", value: "x" },
    { field: "age", type: "NUMERIC", value: "1] | *", upper: "20" },
    { field: "age", type: "NUMERIC", value: "30", upper: "20" },
    { field: "location", type: "GEO", value: "181", latitude: "20", radius: "1", unit: "km" },
    { field: "location", type: "GEO", value: "120", latitude: "20", radius: "0", unit: "km" },
    { field: "name", type: "VECTOR", value: "1" },
  ])("拒绝非法或无法安全表达的条件：%j", (filter) => {
    expect(buildSearchQuery([filter as SearchFilter]).query).toBeNull();
  });

  it("开放范围使用无穷端点；空条件返回全部；限制查询长度", () => {
    expect(buildSearchQuery([{ field: "age", type: "NUMERIC", value: "", upper: "20" }]).query).toBe("@age:[-inf 20]");
    expect(buildSearchQuery([]).query).toBe("*");
    expect(buildSearchQuery([{ field: "name", type: "TEXT", value: "中".repeat(1400) }]).query).toBeNull();
  });

  it("使用 JSON 字段别名，兼容旧响应并隐藏未索引或复杂字段", () => {
    expect(queryBuilderFields([
      { identifier: "$.name", query_name: "name", field_type: "TEXT", sortable: false, no_index: false },
      { identifier: "age", field_type: "NUMERIC", sortable: false, no_index: false },
      { identifier: "$.old", field_type: "TAG", sortable: false, no_index: false },
      { identifier: "hidden", field_type: "TEXT", sortable: false, no_index: true },
    ]).map((field) => field.name)).toEqual(["name", "age"]);
  });
});
