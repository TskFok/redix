import { useState } from "react";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import KeyList from "./KeyList";
import type { KeySummary } from "../../lib/types";

afterEach(cleanup);

const summaries = (keys: string[]): KeySummary[] => keys.map((key) => ({ key, key_type: "string", ttl_ms: -1, size: 3 }));

function Harness({ keys = ["user", "user:1", "user:2", "cache:one"], pattern = "*", loading = false, onPatternChange = vi.fn(), onLoadMore = vi.fn() }: {
  keys?: string[];
  pattern?: string;
  loading?: boolean;
  onPatternChange?: (pattern: string) => void;
  onLoadMore?: () => void;
}) {
  const [selected, setSelected] = useState<string[]>([]);
  const [opened, setOpened] = useState<string | null>(null);
  return <>
    <KeyList pattern={pattern} keyType="" keys={summaries(keys)} selectedKey={opened} selectedKeys={selected}
      hasMore loading={loading} onPatternChange={onPatternChange} onPatternKeyDown={vi.fn()} onKeyTypeChange={vi.fn()}
      onSelect={setOpened} onToggleSelect={(key) => setSelected((current) => current.includes(key) ? current.filter((item) => item !== key) : [...current, key])} onLoadMore={onLoadMore} />
    <output aria-label="已打开的键">{opened}</output>
    <output aria-label="已选择的键">{JSON.stringify(selected)}</output>
  </>;
}

describe("键树浏览", () => {
  it("在平铺和层级之间切换保留准确键名与批量选择，前缀本身也是独立键", () => {
    render(<Harness />);
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

  it("树形仍使用现有搜索与加载更多，追加扫描不清除展开状态", () => {
    const patterns: string[] = [];
    let loads = 0;
    const props = { onPatternChange: (pattern: string) => { patterns.push(pattern); }, onLoadMore: () => { loads += 1; } };
    const { rerender } = render(<Harness {...props} />);
    fireEvent.click(screen.getByRole("button", { name: "树形" }));
    fireEvent.click(screen.getByRole("button", { name: "展开前缀 user:" }));
    fireEvent.click(screen.getByRole("button", { name: "加载更多" }));
    expect(loads).toBe(1);
    rerender(<Harness {...props} keys={["user:1", "user:3"]} />);
    expect(screen.getByRole("button", { name: "user:3" })).toBeInTheDocument();
    fireEvent.change(screen.getByLabelText("键过滤"), { target: { value: "cache:*" } });
    expect(patterns).toEqual(["cache:*"]);
    rerender(<Harness {...props} pattern="cache:*" keys={["cache:one"]} />);
    expect(screen.queryByRole("button", { name: "折叠前缀 user:" })).not.toBeInTheDocument();
  });

  it("加载时树中的键打开和批量选择都禁用", () => {
    const { rerender } = render(<Harness />);
    fireEvent.click(screen.getByRole("button", { name: "树形" }));
    fireEvent.click(screen.getByRole("button", { name: "展开前缀 user:" }));
    rerender(<Harness loading />);
    expect(screen.getByRole("checkbox", { name: "选择键 user:1" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "user:1" })).toBeDisabled();
  });
});

it("可以用多字符分隔符分组，清空分隔符时按完整键名展示", () => {
  render(<Harness keys={["user::one", "user::two"]} />);
  fireEvent.click(screen.getByRole("button", { name: "树形" }));
  fireEvent.change(screen.getByLabelText("键树分隔符"), { target: { value: "::" } });
  fireEvent.click(screen.getByRole("button", { name: "展开前缀 user::" }));
  expect(screen.getByRole("button", { name: "user::one" })).toBeInTheDocument();
  fireEvent.change(screen.getByLabelText("键树分隔符"), { target: { value: "" } });
  expect(screen.queryByRole("button", { name: /展开前缀/ })).not.toBeInTheDocument();
  expect(screen.getByRole("button", { name: "user::two" })).toBeInTheDocument();
});
