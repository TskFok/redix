import { useState } from "react";
import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import Select from "./Select";

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

function ControlledSelect() {
  const [value, setValue] = useState("one");
  return <label>格式<Select value={value} onChange={(event) => setValue(event.target.value)}>
    <option value="one">第一项</option>
    <option value="disabled" disabled>不可用项</option>
    <option value="three">第三项</option>
  </Select></label>;
}

describe("Select", () => {
  it("在裁剪容器外展开菜单，并通过真实 change 事件更新关联的原生选择框", () => {
    const { container } = render(<div style={{ overflow: "hidden" }}><ControlledSelect /></div>);
    const select = screen.getByRole("combobox", { name: "格式" });
    fireEvent.click(select);
    const menu = screen.getByRole("listbox");
    expect(container).not.toContainElement(menu);
    expect(document.body).toContainElement(menu);
    expect(select).toHaveAttribute("aria-expanded", "true");
    fireEvent.click(within(menu).getByRole("option", { name: "第三项" }));
    expect(select).toHaveValue("three");
    expect(screen.queryByRole("listbox")).not.toBeInTheDocument();
    expect(select).toHaveFocus();
  });

  it("方向键预览跳过禁用项，Escape 取消时不提交", () => {
    render(<ControlledSelect />);
    const select = screen.getByRole("combobox");
    select.focus();
    fireEvent.keyDown(select, { key: "ArrowDown" });
    const menu = screen.getByRole("listbox");
    expect(within(menu).getByRole("option", { name: "第三项" })).toHaveAttribute("data-active", "true");
    expect(select).toHaveValue("one");
    fireEvent.keyDown(select, { key: "Escape" });
    expect(select).toHaveValue("one");
    expect(select).toHaveFocus();
    expect(screen.queryByRole("listbox")).not.toBeInTheDocument();
  });

  it("Home、End、Enter 与空格提交，Tab 关闭但不提交预览", () => {
    render(<ControlledSelect />);
    const select = screen.getByRole("combobox");
    fireEvent.keyDown(select, { key: " " });
    fireEvent.keyDown(select, { key: "End" });
    fireEvent.keyDown(select, { key: "Enter" });
    expect(select).toHaveValue("three");
    fireEvent.keyDown(select, { key: "Home" });
    fireEvent.keyDown(select, { key: "Tab" });
    expect(select).toHaveValue("three");
    expect(screen.queryByRole("listbox")).not.toBeInTheDocument();
    fireEvent.keyDown(select, { key: "Home" });
    fireEvent.keyDown(select, { key: " " });
    expect(select).toHaveValue("one");
  });

  it("禁用选项不能点击，失焦与点击外部均关闭菜单", () => {
    render(<><ControlledSelect /><button>外部按钮</button></>);
    const select = screen.getByRole("combobox");
    fireEvent.click(select);
    fireEvent.click(within(screen.getByRole("listbox")).getByRole("option", { name: "不可用项" }));
    expect(select).toHaveValue("one");
    expect(screen.getByRole("listbox")).toBeInTheDocument();
    fireEvent.pointerDown(screen.getByRole("button", { name: "外部按钮" }));
    expect(screen.queryByRole("listbox")).not.toBeInTheDocument();
    fireEvent.click(select);
    fireEvent.blur(select);
    expect(screen.queryByRole("listbox")).not.toBeInTheDocument();
  });

  it("保留空值与数字选项，并兼容原生 fireEvent.change", () => {
    function NumberSelect() {
      const [value, setValue] = useState<string | number>("");
      return <Select aria-label="数量" value={value} onChange={(event) => setValue(event.target.value)}>
        <option value="">全部</option><option value={0}>关闭</option><option value={10}>十条</option>
      </Select>;
    }
    render(<NumberSelect />);
    const select = screen.getByRole("combobox");
    fireEvent.click(select);
    fireEvent.click(within(screen.getByRole("listbox")).getByRole("option", { name: "关闭" }));
    expect(select).toHaveValue("0");
    fireEvent.click(select);
    fireEvent.click(within(screen.getByRole("listbox")).getByRole("option", { name: "全部" }));
    expect(select).toHaveValue("");
    fireEvent.change(select, { target: { value: "10" } });
    expect(select).toHaveValue("10");
  });

  it("键入检索只预览匹配项直到提交", () => {
    render(<Select aria-label="拓扑" defaultValue="standalone">
      <option value="standalone">Standalone</option><option value="sentinel">Sentinel</option><option value="cluster">Cluster</option>
    </Select>);
    const select = screen.getByRole("combobox");
    fireEvent.keyDown(select, { key: "c" });
    expect(select).toHaveValue("standalone");
    expect(within(screen.getByRole("listbox")).getByRole("option", { name: "Cluster" })).toHaveAttribute("data-active", "true");
    fireEvent.keyDown(select, { key: "Enter" });
    expect(select).toHaveValue("cluster");
  });

  it("异步受控回调收到真实 select 事件并在完成后同步值", async () => {
    let finish!: () => void;
    const pending = new Promise<void>((resolve) => { finish = resolve; });
    const targets: EventTarget[] = [];
    function AsyncSelect() {
      const [value, setValue] = useState("a");
      return <Select aria-label="异步" value={value} onChange={async (event) => {
        targets.push(event.target);
        const selected = event.target.value;
        await pending;
        setValue(selected);
      }}><option value="a">A</option><option value="b">B</option></Select>;
    }
    render(<AsyncSelect />);
    const select = screen.getByRole("combobox");
    fireEvent.click(select);
    fireEvent.click(within(screen.getByRole("listbox")).getByRole("option", { name: "B" }));
    expect(targets).toEqual([select]);
    expect(select).toHaveValue("a");
    await act(async () => { finish(); await pending; });
    expect(select).toHaveValue("b");
  });

  it("选项动态移除后不提交旧选项，禁用控件会关闭菜单", () => {
    const { rerender } = render(<Select aria-label="动态" value="a" onChange={() => undefined}><option value="a">A</option><option value="b">B</option></Select>);
    const select = screen.getByRole("combobox");
    fireEvent.click(select);
    fireEvent.keyDown(select, { key: "End" });
    rerender(<Select aria-label="动态" value="a" onChange={() => undefined}><option value="a">A</option><option value="c">C</option></Select>);
    expect(within(screen.getByRole("listbox")).queryByRole("option", { name: "B" })).not.toBeInTheDocument();
    expect(select).toHaveValue("a");
    rerender(<Select aria-label="动态" value="a" disabled><option value="a">A</option></Select>);
    expect(select).toBeDisabled();
    expect(screen.queryByRole("listbox")).not.toBeInTheDocument();
    fireEvent.click(select);
    expect(screen.queryByRole("listbox")).not.toBeInTheDocument();
  });

  it("继承 fieldset 禁用状态，进行中的选择在父级禁用后取消", () => {
    const { rerender } = render(<fieldset><ControlledSelect /></fieldset>);
    const select = screen.getByRole("combobox");
    fireEvent.click(select);
    expect(screen.getByRole("listbox")).toBeInTheDocument();
    rerender(<fieldset disabled><ControlledSelect /></fieldset>);
    expect(select).toBeDisabled();
    expect(screen.queryByRole("listbox")).not.toBeInTheDocument();
    fireEvent.click(select);
    expect(screen.queryByRole("listbox")).not.toBeInTheDocument();
    expect(select).toHaveValue("one");
  });

  it("底部空间不足时菜单向上展开且保持在视口内", () => {
    render(<ControlledSelect />);
    const select = screen.getByRole("combobox");
    vi.spyOn(select, "getBoundingClientRect").mockReturnValue({ x: 10, y: 700, top: 700, bottom: 740, left: 10, right: 210, width: 200, height: 40, toJSON: () => ({}) });
    fireEvent.click(select);
    const menu = screen.getByRole("listbox");
    expect(menu.style.position).toBe("fixed");
    expect(Number.parseFloat(menu.style.top)).toBeLessThan(700);
    expect(Number.parseFloat(menu.style.top)).toBeGreaterThanOrEqual(8);
    expect(Number.parseFloat(menu.style.width)).toBe(200);
  });

  it("首次打开长列表时在限高生效后滚动到已选项", () => {
    // JSDOM has no layout; model 20 rows before and after the menu height is capped.
    vi.spyOn(HTMLElement.prototype, "offsetTop", "get").mockImplementation(function(this: HTMLElement) {
      return this.getAttribute("role") === "option" ? Array.from(this.parentElement!.children).indexOf(this) * 40 : 0;
    });
    vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockReturnValue(40);
    vi.spyOn(HTMLElement.prototype, "clientHeight", "get").mockImplementation(function(this: HTMLElement) {
      return this.style.maxHeight ? Number.parseFloat(this.style.maxHeight) : 800;
    });
    render(<Select aria-label="长列表" defaultValue={19}>{Array.from({ length: 20 }, (_, index) => <option key={index} value={index}>选项 {index}</option>)}</Select>);
    fireEvent.click(screen.getByRole("combobox"));
    expect(screen.getByRole("listbox").scrollTop).toBe(520);
  });
});
