import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import ShortcutPalette, { type ShortcutAction } from "./ShortcutPalette";

afterEach(cleanup);
function actions() {
  return [
    { id: "connections", label: "连接管理", description: "Connections", shortcut: { key: "1", label: "Ctrl/Cmd+1" }, run: vi.fn() },
    { id: "browser", label: "Browser", description: "键浏览", unavailable: () => "请先连接 Redis", shortcut: { key: "2", label: "Ctrl/Cmd+2" }, run: vi.fn() },
    { id: "settings", label: "设置", description: "应用偏好", shortcut: { key: ",", label: "Ctrl/Cmd+," }, run: vi.fn() },
  ] satisfies ShortcutAction[];
}

describe("快捷键与操作", () => {
  it("Ctrl与Cmd+K打开搜索，Escape/再次K关闭并恢复输入焦点", () => {
    render(<><input aria-label="原输入" /><ShortcutPalette actions={actions()} /></>);
    const original = screen.getByLabelText("原输入");
    for (const modifier of [{ ctrlKey: true }, { metaKey: true }]) {
      original.focus();
      fireEvent.keyDown(original, { key: "k", ...modifier });
      const search = screen.getByRole("combobox", { name: "搜索操作" });
      expect(search).toHaveFocus();
      fireEvent.keyDown(search, { key: "Escape" });
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
      expect(original).toHaveFocus();
    }
    fireEvent.keyDown(original, { key: "k", ctrlKey: true });
    fireEvent.keyDown(screen.getByRole("combobox"), { key: "k", ctrlKey: true });
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(original).toHaveFocus();
  });

  it("搜索名称和描述，上下键跳过禁用项，Enter只运行选中操作", () => {
    const items = actions();
    render(<ShortcutPalette actions={items} />);
    fireEvent.click(screen.getByRole("button", { name: "快捷键与操作" }));
    const search = screen.getByRole("combobox");
    expect(screen.getByRole("option", { name: /Browser/ })).toHaveAttribute("aria-disabled", "true");
    fireEvent.keyDown(search, { key: "ArrowDown" });
    expect(screen.getByRole("option", { name: /设置/ })).toHaveAttribute("aria-selected", "true");
    fireEvent.keyDown(search, { key: "ArrowUp" });
    expect(screen.getByRole("option", { name: /连接管理/ })).toHaveAttribute("aria-selected", "true");
    fireEvent.change(search, { target: { value: "偏好" } });
    expect(screen.getAllByRole("option")).toHaveLength(1);
    fireEvent.keyDown(search, { key: "Enter" });
    expect(items[2].run).toHaveBeenCalledOnce();
    expect(items[0].run).not.toHaveBeenCalled();
    expect(items[1].run).not.toHaveBeenCalled();
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("禁用与无匹配搜索不接受鼠标、Enter或直接快捷键", () => {
    const items = actions();
    render(<ShortcutPalette actions={items} />);
    fireEvent.keyDown(window, { key: "2", ctrlKey: true });
    expect(items[1].run).not.toHaveBeenCalled();
    fireEvent.keyDown(window, { key: "k", ctrlKey: true });
    const search = screen.getByRole("combobox");
    fireEvent.change(search, { target: { value: "browser" } });
    fireEvent.click(screen.getByRole("option"));
    fireEvent.keyDown(search, { key: "Enter" });
    expect(items[1].run).not.toHaveBeenCalled();
    expect(screen.getByRole("dialog")).toBeInTheDocument();
    fireEvent.change(search, { target: { value: "找不到" } });
    expect(screen.getByRole("status")).toHaveTextContent("没有匹配的操作");
    fireEvent.keyDown(search, { key: "ArrowDown" });
    fireEvent.keyDown(search, { key: "Enter" });
    expect(items.every((item) => item.run.mock.calls.length === 0)).toBe(true);
  });

  it("普通输入、编辑区域、IME、重复与Alt组合不触发全局导航", () => {
    const items = actions();
    render(<><input aria-label="输入" /><div contentEditable role="textbox" aria-label="编辑区" /><ShortcutPalette actions={items} /></>);
    const input = screen.getByLabelText("输入");
    fireEvent.keyDown(input, { key: "1", ctrlKey: true });
    fireEvent.keyDown(screen.getByLabelText("编辑区"), { key: "1", metaKey: true });
    fireEvent.keyDown(window, { key: "1" });
    for (const blocked of [{ isComposing: true }, { keyCode: 229 }, { repeat: true }, { altKey: true }]) {
      fireEvent.keyDown(window, { key: "1", ctrlKey: true, ...blocked });
      fireEvent.keyDown(window, { key: "k", ctrlKey: true, ...blocked });
    }
    expect(items[0].run).not.toHaveBeenCalled();
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    fireEvent.keyDown(window, { key: "1", ctrlKey: true });
    expect(items[0].run).toHaveBeenCalledOnce();
    fireEvent.keyDown(window, { key: ",", metaKey: true });
    expect(items[2].run).toHaveBeenCalledOnce();
  });

  it("输入法候选确认不执行或关闭面板，Tab焦点留在弹层", () => {
    const items = actions();
    render(<ShortcutPalette actions={items} />);
    fireEvent.keyDown(window, { key: "k", ctrlKey: true });
    const dialog = screen.getByRole("dialog");
    const search = screen.getByRole("combobox");
    fireEvent.compositionStart(search);
    fireEvent.keyDown(search, { key: "Enter" });
    fireEvent.keyDown(search, { key: "Escape" });
    expect(items[0].run).not.toHaveBeenCalled();
    expect(dialog).toBeInTheDocument();
    fireEvent.compositionEnd(search);
    fireEvent.keyDown(search, { key: "Tab", shiftKey: true });
    const close = within(dialog).getByRole("button", { name: "关闭操作面板" });
    expect(close).toHaveFocus();
    fireEvent.keyDown(close, { key: "Tab" });
    expect(search).toHaveFocus();
    fireEvent.click(close);
    expect(screen.getByRole("button", { name: "快捷键与操作" })).toHaveFocus();
  });

  it("新连接状态立即更新禁用守卫，卸载移除全局监听", () => {
    const run = vi.fn();
    const item = { id: "browser", label: "Browser", description: "键浏览", shortcut: { key: "2", label: "Ctrl/Cmd+2" }, run };
    const view = render(<ShortcutPalette actions={[item]} />);
    fireEvent.keyDown(window, { key: "k", ctrlKey: true });
    view.rerender(<ShortcutPalette actions={[{ ...item, unavailable: () => "请先连接 Redis" }]} />);
    fireEvent.keyDown(screen.getByRole("combobox"), { key: "Enter" });
    expect(run).not.toHaveBeenCalled();
    view.unmount();
    fireEvent.keyDown(window, { key: "2", ctrlKey: true });
    expect(run).not.toHaveBeenCalled();
  });

  it("鼠标关闭组字中的面板不会让后续快捷键失效", () => {
    render(<ShortcutPalette actions={actions()} />);
    fireEvent.keyDown(window, { key: "k", ctrlKey: true });
    fireEvent.compositionStart(screen.getByRole("combobox"));
    fireEvent.click(screen.getByRole("button", { name: "关闭操作面板" }));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    fireEvent.keyDown(window, { key: "k", ctrlKey: true });
    expect(screen.getByRole("dialog")).toBeInTheDocument();
  });
});
