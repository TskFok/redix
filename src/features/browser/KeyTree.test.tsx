import { useState } from "react";
import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import KeyList from "./KeyList";
import type { KeySummary } from "../../lib/types";

afterEach(cleanup);

const summaries = (keys: string[]): KeySummary[] => keys.map((key) => ({ key, key_type: "string", ttl_ms: -1, size: 3 }));

function Harness({ keys = ["user", "user:1", "user:2", "cache:one"], pattern = "*", loading = false, onPatternChange = vi.fn() }: {
  keys?: string[];
  pattern?: string;
  loading?: boolean;
  onPatternChange?: (pattern: string) => void;
}) {
  const [selected, setSelected] = useState<string[]>([]);
  const [opened, setOpened] = useState<string | null>(null);
  return <>
    <KeyList pattern={pattern} keyType="" keys={summaries(keys)} selectedKey={opened} selectedKeys={selected}
      loading={loading} scanFailed={false} onPatternChange={onPatternChange} onPatternKeyDown={vi.fn()} onKeyTypeChange={vi.fn()}
      onSelect={setOpened} onToggleSelect={(key) => setSelected((current) => current.includes(key) ? current.filter((item) => item !== key) : [...current, key])} />
    <output aria-label="已打开的键">{opened}</output>
    <output aria-label="已选择的键">{JSON.stringify(selected)}</output>
  </>;
}

describe("键树浏览", () => {
  it("筛选默认收起，打开后聚焦键过滤且显示方式仍在主面板", () => {
    render(<Harness />);

    expect(screen.queryByRole("dialog", { name: "SCAN 筛选" })).not.toBeInTheDocument();
    expect(screen.queryByLabelText("键过滤")).not.toBeInTheDocument();
    expect(screen.queryByLabelText("类型过滤")).not.toBeInTheDocument();
    expect(screen.queryByLabelText("键树分隔符")).not.toBeInTheDocument();
    const viewSwitch = screen.getByRole("group", { name: "键显示方式" });

    fireEvent.click(screen.getByRole("button", { name: "筛选" }));

    const dialog = screen.getByRole("dialog", { name: "SCAN 筛选" });
    expect(dialog).toHaveAttribute("aria-modal", "true");
    expect(within(dialog).getByLabelText("键过滤")).toHaveFocus();
    expect(within(dialog).getByLabelText("类型过滤")).toBeInTheDocument();
    expect(within(dialog).getByLabelText("键树分隔符")).toHaveValue(":");
    expect(within(dialog).getAllByRole("button", { name: "关闭筛选" })).toHaveLength(1);
    expect(dialog).not.toContainElement(viewSwitch);
  });

  it.each(["关闭按钮", "Escape", "遮罩"])("通过%s关闭筛选后恢复入口焦点", (method) => {
    render(<Harness />);
    const trigger = screen.getByRole("button", { name: "筛选" });
    trigger.focus();
    fireEvent.click(trigger);
    const dialog = screen.getByRole("dialog", { name: "SCAN 筛选" });

    if (method === "关闭按钮") fireEvent.click(within(dialog).getByRole("button", { name: "关闭筛选" }));
    else if (method === "Escape") fireEvent.keyDown(document.activeElement!, { key: "Escape" });
    else fireEvent.click(dialog.parentElement!);

    expect(screen.queryByRole("dialog", { name: "SCAN 筛选" })).not.toBeInTheDocument();
    expect(screen.queryByLabelText("键过滤")).not.toBeInTheDocument();
    expect(trigger).toHaveFocus();
  });

  it("类型下拉打开时 Escape 先收起选项，再关闭筛选弹窗", () => {
    render(<Harness />);
    const trigger = screen.getByRole("button", { name: "筛选" });
    fireEvent.click(trigger);
    const select = screen.getByRole("combobox", { name: "类型过滤" });
    select.focus();
    fireEvent.keyDown(select, { key: "ArrowDown" });
    expect(screen.getByRole("listbox", { name: "类型过滤" })).toBeInTheDocument();

    fireEvent.keyDown(select, { key: "Escape" });

    expect(screen.queryByRole("listbox", { name: "类型过滤" })).not.toBeInTheDocument();
    expect(screen.getByRole("dialog", { name: "SCAN 筛选" })).toBeInTheDocument();
    expect(select).toHaveFocus();
    expect(select).toHaveValue("");
    fireEvent.keyDown(select, { key: "Escape" });
    expect(screen.queryByRole("dialog", { name: "SCAN 筛选" })).not.toBeInTheDocument();
    expect(trigger).toHaveFocus();
  });

  it("首次展示自动按前缀分类，同层目录优先并按名称排序", () => {
    render(<Harness keys={["zebra", "user:2", "cache:z", "alpha", "user:1", "cache:a", "user:profile:name"]} />);

    const tree = screen.getByRole("list", { name: "Redis 键树" });
    expect(within(tree).getAllByRole("button").map((button) => button.getAttribute("aria-label"))).toEqual([
      "展开前缀 cache:", "展开前缀 user:", "alpha", "zebra",
    ]);
    expect(screen.getByRole("button", { name: "展开前缀 user:" })).toHaveTextContent("3");
    fireEvent.click(screen.getByRole("button", { name: "展开前缀 user:" }));
    const users = screen.getByRole("list", { name: "前缀 user: 的键" });
    expect(within(users).getAllByRole("button").map((button) => button.getAttribute("aria-label"))).toEqual([
      "展开前缀 user:profile:", "user:1", "user:2",
    ]);
    fireEvent.click(screen.getByRole("button", { name: "展开前缀 user:profile:" }));
    fireEvent.click(screen.getByRole("button", { name: "user:profile:name" }));
    expect(screen.getByLabelText("已打开的键")).toHaveTextContent("user:profile:name");
  });

  it("在平铺和层级之间切换保留准确键名与批量选择，前缀本身也是独立键", () => {
    render(<Harness />);
    fireEvent.click(screen.getByRole("button", { name: "平铺" }));
    fireEvent.click(screen.getByRole("checkbox", { name: "选择键 user:1" }));
    fireEvent.click(screen.getByRole("button", { name: "树形" }));
    expect(screen.queryByRole("button", { name: "user:1" })).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "展开前缀 user:" }));
    expect(screen.getByRole("checkbox", { name: "选择键 user:1" })).toBeChecked();
    fireEvent.click(screen.getByRole("button", { name: "user:2" }));
    expect(screen.getByLabelText("已打开的键")).toHaveTextContent("user:2");
    fireEvent.click(screen.getByRole("checkbox", { name: "选择键 user" }));
    expect(screen.getByLabelText("已选择的键")).toHaveTextContent('["user:1","user"]');
    fireEvent.click(screen.getByRole("button", { name: "平铺" }));
    expect(screen.getByRole("checkbox", { name: "选择键 user:1" })).toBeChecked();
    expect(screen.getByRole("button", { name: "user:2" })).toHaveAttribute("aria-pressed", "true");
  });

  it("空段、连续分隔符和原型属性不发生碰撞或丢失", () => {
    render(<Harness keys={["", ":lead", "a:", "a::b", "__proto__:x", "constructor:y"]} />);
    fireEvent.click(screen.getByRole("button", { name: "树形" }));
    for (const prefix of [":", "a:", "a::", "__proto__:", "constructor:"]) {
      fireEvent.click(screen.getByRole("button", { name: `展开前缀 ${prefix}` }));
    }
    for (const key of ["", ":lead", "a:", "a::b", "__proto__:x", "constructor:y"]) {
      fireEvent.click(screen.getByRole("checkbox", { name: `选择键 ${key}`.trim() }));
    }
    expect(screen.getByLabelText("已选择的键")).toHaveTextContent('["",":lead","a:","a::b","__proto__:x","constructor:y"]');
  });

  it("树形保留搜索入口，新增键不清除展开状态", () => {
    const patterns: string[] = [];
    const props = { onPatternChange: (pattern: string) => { patterns.push(pattern); } };
    const { rerender } = render(<Harness {...props} />);
    fireEvent.click(screen.getByRole("button", { name: "树形" }));
    fireEvent.click(screen.getByRole("button", { name: "展开前缀 user:" }));
    expect(screen.queryByRole("button", { name: "加载更多" })).not.toBeInTheDocument();
    rerender(<Harness {...props} keys={["user", "user:1", "user:2", "cache:one", "user:3", "account:one"]} />);
    expect(screen.getByRole("button", { name: "user:3" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "折叠前缀 user:" })).toHaveTextContent("3");
    expect(screen.getAllByRole("button", { name: /(?:展开|折叠)前缀/ }).map((button) => button.getAttribute("aria-label"))).toEqual([
      "展开前缀 account:", "展开前缀 cache:", "折叠前缀 user:",
    ]);
    fireEvent.click(screen.getByRole("button", { name: "筛选" }));
    fireEvent.change(screen.getByLabelText("键过滤"), { target: { value: "cache:*" } });
    expect(patterns).toEqual(["cache:*"]);
    fireEvent.click(screen.getByRole("button", { name: "关闭筛选" }));
    rerender(<Harness {...props} pattern="cache:*" keys={["cache:one"]} />);
    expect(screen.queryByRole("button", { name: "折叠前缀 user:" })).not.toBeInTheDocument();
  });

  it("全量扫描等待时隐藏旧树和勾选，仅显示扫描提示", () => {
    const { rerender } = render(<Harness />);
    fireEvent.click(screen.getByRole("button", { name: "树形" }));
    fireEvent.click(screen.getByRole("button", { name: "展开前缀 user:" }));
    rerender(<Harness loading />);
    expect(screen.getByText("正在扫描键…")).toBeInTheDocument();
    expect(screen.queryByRole("list", { name: "Redis 键树" })).not.toBeInTheDocument();
    expect(screen.queryByRole("checkbox", { name: "选择键 user:1" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "user:1" })).not.toBeInTheDocument();
  });
});

it("可以用多字符分隔符分组，清空分隔符时按完整键名展示", () => {
  render(<Harness keys={["user::one", "user::two"]} />);
  fireEvent.click(screen.getByRole("button", { name: "树形" }));
  fireEvent.click(screen.getByRole("button", { name: "筛选" }));
  fireEvent.change(screen.getByLabelText("键树分隔符"), { target: { value: "::" } });
  fireEvent.click(screen.getByRole("button", { name: "关闭筛选" }));
  fireEvent.click(screen.getByRole("button", { name: "展开前缀 user::" }));
  expect(screen.getByRole("button", { name: "user::one" })).toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "筛选" }));
  expect(screen.getByLabelText("键树分隔符")).toHaveValue("::");
  fireEvent.change(screen.getByLabelText("键树分隔符"), { target: { value: "" } });
  fireEvent.click(screen.getByRole("button", { name: "关闭筛选" }));
  expect(screen.queryByRole("button", { name: /展开前缀/ })).not.toBeInTheDocument();
  expect(screen.getByRole("button", { name: "user::two" })).toBeInTheDocument();
});
