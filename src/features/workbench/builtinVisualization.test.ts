import { describe, expect, it } from "vitest";
import { parseVisualization } from "./visualizationData";

describe("Workbench 本地可视化适配", () => {
  it("仅识别实际模块命令，将逆序时序点按时间排序并保留名称", () => {
    expect(parseVisualization("GET key", [[1000, "2"]])).toBeNull();
    expect(parseVisualization('TS.REVRANGE "cpu node" - +', [[2000, "3"], [1000, "2"]]))
      .toMatchObject({ kind: "timeseries", series: [{ name: "cpu node", points: [{ x: 1000, y: 2 }, { x: 2000, y: 3 }] }] });
  });
  it("读取 MRANGE 多序列，不将标签误当样本", () => {
    expect(parseVisualization("TS.MRANGE - + WITHLABELS FILTER host=a", [
      ["cpu", [["host", "a"]], [[1000, "2"]]], ["mem", [], [[1000, 3]]],
    ])).toMatchObject({ kind: "timeseries", series: [{ name: "cpu" }, { name: "mem" }], total: 2 });
  });
  it("拒绝坏形状、非数值、非有限数和超范围时间", () => {
    for (const value of [null, [[1, null]], [[1, ""]], [[1, "NaN"]], [[1, "1e400"]], [[1.5, 2]], [[Number.MAX_SAFE_INTEGER, 2]]]) {
      expect(parseVisualization("TS.RANGE cpu - +", value)).toMatchObject({ error: expect.any(String) });
    }
  });
  it("样本与序列上限有明确错误，避免静默丢点", () => {
    expect(parseVisualization("TS.RANGE cpu - +", Array.from({ length: 2001 }, (_, i) => [i, 1])))
      .toMatchObject({ error: expect.stringContaining("2000") });
    expect(parseVisualization("TS.MRANGE - + FILTER x=y", Array.from({ length: 21 }, (_, i) => [String(i), [], []])))
      .toMatchObject({ error: expect.stringContaining("20") });
  });
  it("拒绝过长数值字符串，数值验证开销有界", () => {
    expect(parseVisualization("TS.RANGE cpu - +", [[1, "9".repeat(16000) + "x"]]))
      .toMatchObject({ error: expect.any(String) });
  });
  it("超长序列名称不会按数据点数放大到图表 DOM", () => {
    expect(parseVisualization("TS.MRANGE - + FILTER x=y", [["k".repeat(100000), [], [[1, 2]]]]))
      .toMatchObject({ series: [{ name: "k".repeat(256) + "…" }] });
  });
  it("GEOPOS 绑定引号成员，缺失成员不落在原点", () => {
    expect(parseVisualization('GEOPOS city "San Francisco" missing', [["-122.4", "37.7"], null]))
      .toMatchObject({ kind: "geo", missing: ["missing"], points: [{ name: "San Francisco", x: -122.4, y: 37.7 }] });
  });
  it("GEOSEARCH WITHCOORD 同时带距离和 hash，坐标顺序固定", () => {
    expect(parseVisualization("GEOSEARCH city FROMMEMBER WITHCOORD BYRADIUS 10 km WITHDIST WITHHASH WITHCOORD", [["a", "0.5", 123, ["2", "3"]]]))
      .toMatchObject({ kind: "geo", points: [{ name: "a", x: 2, y: 3 }] });
    expect(parseVisualization("GEOSEARCH city FROMMEMBER WITHCOORD BYRADIUS 10 km", ["a"]))
      .toMatchObject({ error: expect.stringContaining("WITHCOORD") });
  });
  it("拒绝非法坐标与结果错位，不给写入命令绘制推测结果", () => {
    expect(parseVisualization("GEOPOS city a", [[181, 1]])).toMatchObject({ error: expect.any(String) });
    expect(parseVisualization("GEOPOS city a b", [[1, 2]])).toMatchObject({ error: expect.any(String) });
    expect(parseVisualization("GEOSEARCHSTORE dest src FROMMEMBER a BYRADIUS 1 km", 5)).toBeNull();
    expect(parseVisualization('TS.RANGE "unterminated - +', [])).toBeNull();
  });
});
