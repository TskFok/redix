import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import InstanceTrends, { appendInstanceSample, type InstanceSample } from "./InstanceTrends";

afterEach(cleanup);
const sample = (at: number): InstanceSample => ({ at, memory: at, ops: at, clients: 2 });
describe("实例指标趋势", () => {
  it("样本限制120条并保留不可用间隙", () => {
    let samples: InstanceSample[] = [];
    for (let i = 0; i < 125; i++) samples = appendInstanceSample(samples, sample(i));
    expect(samples).toHaveLength(120);
    expect(samples[0].at).toBe(5);
    expect(appendInstanceSample(samples, { at: 125, memory: null, ops: null, clients: null }).at(-1)?.memory).toBeNull();
  });
  it("不跨不可用样本连线，并允许切换指标查看精确值", () => {
    const { container } = render(<InstanceTrends samples={[sample(1000), { at: 2000, memory: null, ops: 20, clients: null }, sample(3000)]} />);
    expect(screen.getByRole("img", { name: "已用内存趋势" })).toBeInTheDocument();
    expect(container.querySelectorAll("polyline")).toHaveLength(2);
    expect(screen.getByRole("table")).toHaveTextContent("不可用");
    fireEvent.change(screen.getByRole("combobox", { name: "趋势指标" }), { target: { value: "ops" } });
    expect(screen.getByRole("img", { name: "操作数趋势" })).toBeInTheDocument();
    expect(container.querySelectorAll("polyline")).toHaveLength(1);
    expect(screen.getByRole("table")).toHaveTextContent("20");
  });
});
