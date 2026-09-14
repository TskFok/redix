import { fireEvent, render, screen, cleanup } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import KeyList from "./KeyList";

afterEach(cleanup);

const props = {
  pattern: "*", keyType: "", selectedKey: null, selectedKeys: [], loading: false, scanFailed: false,
  onPatternChange: vi.fn(), onPatternKeyDown: vi.fn(), onKeyTypeChange: vi.fn(),
  onSelect: vi.fn(), onToggleSelect: vi.fn(),
};

it.each(["平铺", "树形"])("%s 十万条数据只挂载可视行，滚动后仍可选择末尾键", (view) => {
  const keys = Array.from({ length: 100_000 }, (_, index) => ({
    key: `key-${String(index).padStart(5, "0")}`,
  }));
  const opened: string[] = [];
  const { container } = render(<KeyList {...props} keys={keys} onSelect={(key) => opened.push(key)} />);
  fireEvent.click(screen.getByRole("button", { name: view }));
  expect(container.querySelectorAll(".key-row").length).toBeLessThan(100);
  expect(screen.getByRole("button", { name: "key-00000" })).toBeInTheDocument();
  const viewport = screen.getByRole("list", { name: view === "平铺" ? "Redis 键列表" : "Redis 键树" });
  fireEvent.scroll(viewport, { target: { scrollTop: 100_000 * 48 - 480 } });
  fireEvent.click(screen.getByRole("button", { name: "key-99999" }));
  expect(opened).toEqual(["key-99999"]);
  expect(container.querySelectorAll(".key-row").length).toBeLessThan(100);
});

it("键盘可跨越未挂载的行，滚动离开后仍能再次定位末尾", () => {
  const keys = Array.from({ length: 2000 }, (_, index) => ({ key: `key-${index}`, key_type: "string" }));
  render(<KeyList {...props} keys={keys} selectedKeys={["key-1999"]} />);
  fireEvent.click(screen.getByRole("button", { name: "平铺" }));
  const viewport = screen.getByRole("list", { name: "Redis 键列表" });
  fireEvent.keyDown(viewport, { key: "End" });
  expect(screen.getByRole("button", { name: "key-1999" })).toHaveFocus();
  expect(screen.getByRole("checkbox", { name: "选择键 key-1999" })).toBeChecked();
  fireEvent.scroll(viewport, { target: { scrollTop: 0 } });
  fireEvent.keyDown(viewport, { key: "End" });
  expect(screen.getByRole("button", { name: "key-1999" })).toHaveFocus();
  fireEvent.keyDown(document.activeElement!, { key: "Home" });
  expect(screen.getByRole("button", { name: "key-0" })).toHaveFocus();
  fireEvent.keyDown(document.activeElement!, { key: "ArrowDown" });
  expect(screen.getByRole("button", { name: "key-1" })).toHaveFocus();
});

it("滚动回弹产生负偏移时仍限制挂载行数", () => {
  const keys = Array.from({ length: 2000 }, (_, index) => ({ key: `key-${index}`, key_type: "string" }));
  const { container } = render(<KeyList {...props} keys={keys} />);
  fireEvent.click(screen.getByRole("button", { name: "平铺" }));
  fireEvent.scroll(screen.getByRole("list", { name: "Redis 键列表" }), { target: { scrollTop: -2000 } });
  expect(container.querySelectorAll(".key-row").length).toBeLessThan(100);
  expect(screen.getByRole("button", { name: "key-0" })).toBeInTheDocument();
});

it("展开大目录也保持有限 DOM，收起或换成少量结果后不出现空白列表", () => {
  const keys = Array.from({ length: 2000 }, (_, index) => ({
    key: `user:${String(index).padStart(5, "0")}`, key_type: "hash",
  }));
  const { container, rerender } = render(<KeyList {...props} keys={keys} />);
  fireEvent.click(screen.getByRole("button", { name: "展开前缀 user:" }));
  expect(container.querySelectorAll(".key-row").length).toBeLessThan(100);
  const viewport = screen.getByRole("list", { name: "Redis 键树" });
  fireEvent.scroll(viewport, { target: { scrollTop: 2001 * 48 - 480 } });
  expect(screen.getByRole("button", { name: "user:01999" })).toBeInTheDocument();
  rerender(<KeyList {...props} keys={[keys[0]]} />);
  expect(screen.getByRole("button", { name: "user:00000" })).toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "折叠前缀 user:" }));
  expect(screen.getByRole("button", { name: "展开前缀 user:" })).toBeInTheDocument();
});

it("默认扫描行只展示键名，不显示未知类型或其他元数据", () => {
  const { container } = render(<KeyList {...props} keys={[
    { key: "sample" },
  ]} />);
  expect(screen.getByRole("button", { name: "sample" })).toHaveTextContent("sample");
  expect(container.querySelector(".key-row-meta")).toBeNull();
  expect(container.querySelector(".key-row-type")).toBeNull();
});
