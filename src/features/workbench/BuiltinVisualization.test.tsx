import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import BuiltinVisualization from "./BuiltinVisualization";

afterEach(cleanup);
async function openChart() {
  fireEvent.click(screen.getByText("本地可视化"));
  await waitFor(() => expect(screen.getByRole("img")).toBeInTheDocument());
}
describe("内置结果图表", () => {
  it("保留精确样本表，可选择多序列，极值不会生成 Infinity 坐标", async () => {
    const { container } = render(<BuiltinVisualization command="TS.MRANGE - + FILTER x=y" value={[
      ["cpu", [], [[1, -Number.MAX_VALUE], [2, Number.MAX_VALUE]]], ["mem", [], [[1, 3]]],
    ]} />);
    await openChart();
    expect(screen.getByRole("table", { name: "时间序列数据" })).toHaveTextContent("cpu");
    expect(container.querySelector("svg")?.outerHTML).not.toMatch(/Infinity|NaN|∞/);
    fireEvent.change(screen.getByRole("combobox", { name: "显示序列" }), { target: { value: "1" } });
    expect(screen.getByRole("table")).toHaveTextContent("mem");
    expect(screen.getByRole("table")).not.toHaveTextContent("cpu");
  });
  it("坐标空值明确列为缺失，数据表分页限制 DOM 行数", async () => {
    const command = `GEOPOS cities ${Array.from({ length: 102 }, (_, i) => `m${i}`).join(" ")}`;
    render(<BuiltinVisualization command={command} value={[...Array.from({ length: 101 }, () => [1, 2]), null]} />);
    await openChart();
    expect(screen.getByText("缺少坐标的成员：m101")).toBeInTheDocument();
    expect(screen.getAllByRole("row")).toHaveLength(101);
    fireEvent.click(screen.getByRole("button", { name: "下一页数据" }));
    expect(screen.getAllByRole("row")).toHaveLength(2);
    expect(screen.getByRole("table")).toHaveTextContent("m100");
  });
});
