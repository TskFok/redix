import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";

import { useConfirmDialog } from "./useConfirmDialog";

afterEach(cleanup);

function renderConfirmation(initialScope = "connection-a:0") {
  let dialog!: ReturnType<typeof useConfirmDialog>;
  function Harness({ scope }: { scope: string }) {
    dialog = useConfirmDialog(scope);
    return <div style={{ overflow: "hidden" }}><button>删除入口</button>{dialog.confirmationDialog}</div>;
  }
  const view = render(<Harness scope={initialScope} />);
  return {
    ...view,
    get confirm() { return dialog.confirm; },
    changeScope(scope: string) { view.rerender(<Harness scope={scope} />); },
    request(message = "确定删除 user:1？") {
      let answer!: Promise<boolean>;
      act(() => { answer = dialog.confirm(message); });
      return answer;
    },
  };
}

describe("useConfirmDialog", () => {
  it("在页面裁剪区域外显示确认提示，只有明确确认后才返回 true", async () => {
    const view = renderConfirmation();
    const trigger = screen.getByRole("button", { name: "删除入口" });
    trigger.focus();
    const answers: boolean[] = [];
    const answer = view.request();
    void answer.then((value) => answers.push(value));
    const dialog = screen.getByRole("alertdialog", { name: "确认删除" });
    expect(view.container).not.toContainElement(dialog);
    expect(document.body).toContainElement(dialog);
    expect(dialog).toHaveAttribute("aria-modal", "true");
    expect(dialog).toHaveAccessibleDescription("确定删除 user:1？");
    expect(within(dialog).getByRole("button", { name: "取消" })).toHaveFocus();
    await act(async () => { await Promise.resolve(); });
    expect(answers).toEqual([]);

    fireEvent.click(within(dialog).getByRole("button", { name: "确认删除" }));
    await expect(answer).resolves.toBe(true);
    expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
    expect(trigger).toHaveFocus();
  });

  it.each(["取消按钮", "Escape", "遮罩"])("通过%s取消时返回 false 并归还焦点", async (method) => {
    const view = renderConfirmation();
    const trigger = screen.getByRole("button", { name: "删除入口" });
    trigger.focus();
    const answer = view.request();
    const dialog = screen.getByRole("alertdialog");
    if (method === "取消按钮") fireEvent.click(within(dialog).getByRole("button", { name: "取消" }));
    else if (method === "Escape") fireEvent.keyDown(document.activeElement!, { key: "Escape" });
    else fireEvent.click(dialog.parentElement!);

    await expect(answer).resolves.toBe(false);
    expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
    expect(trigger).toHaveFocus();
  });

  it("Tab 和 Shift+Tab 在弹窗按钮间循环，弹窗内点击不会误取消", async () => {
    const view = renderConfirmation();
    const answer = view.request();
    const dialog = screen.getByRole("alertdialog");
    const cancel = within(dialog).getByRole("button", { name: "取消" });
    const accept = within(dialog).getByRole("button", { name: "确认删除" });
    fireEvent.keyDown(cancel, { key: "Tab" });
    expect(accept).toHaveFocus();
    fireEvent.keyDown(accept, { key: "Tab" });
    expect(cancel).toHaveFocus();
    fireEvent.keyDown(cancel, { key: "Tab", shiftKey: true });
    expect(accept).toHaveFocus();
    fireEvent.keyDown(accept, { key: "Tab", shiftKey: true });
    expect(cancel).toHaveFocus();
    fireEvent.click(within(dialog).getByText("确定删除 user:1？"));
    expect(dialog).toBeInTheDocument();
    fireEvent.click(accept);
    await expect(answer).resolves.toBe(true);
  });

  it("弹窗打开期间隔离页面快捷键，关闭后恢复页面按键处理", async () => {
    const keys: string[] = [];
    const pageShortcut = (event: KeyboardEvent) => { keys.push(`${event.type}:${event.key}`); };
    window.addEventListener("keydown", pageShortcut);
    window.addEventListener("keyup", pageShortcut);
    try {
      const view = renderConfirmation();
      const answer = view.request();
      const cancel = screen.getByRole("button", { name: "取消" });
      fireEvent.keyDown(cancel, { key: "k", metaKey: true });
      fireEvent.keyUp(cancel, { key: "k", metaKey: true });
      expect(keys).toEqual([]);
      fireEvent.click(cancel);
      await expect(answer).resolves.toBe(false);
      fireEvent.keyDown(document.body, { key: "k", metaKey: true });
      expect(keys).toEqual(["keydown:k"]);
    } finally {
      window.removeEventListener("keydown", pageShortcut);
      window.removeEventListener("keyup", pageShortcut);
    }
  });

  it("连续请求只保留首个确认，后续请求立即取消", async () => {
    const view = renderConfirmation();
    let first!: Promise<boolean>;
    let second!: Promise<boolean>;
    act(() => {
      first = view.confirm("删除首个键？");
      second = view.confirm("删除第二个键？");
    });
    await expect(second).resolves.toBe(false);
    expect(screen.getAllByRole("alertdialog")).toHaveLength(1);
    expect(screen.getByRole("alertdialog")).toHaveAccessibleDescription("删除首个键？");
    fireEvent.click(screen.getByRole("button", { name: "确认删除" }));
    await expect(first).resolves.toBe(true);
  });

  it("切换作用域取消待定确认，返回原作用域后也拒绝旧回调", async () => {
    const view = renderConfirmation();
    const oldConfirm = view.confirm;
    const oldAnswer = view.request();
    view.changeScope("connection-b:1");
    await expect(oldAnswer).resolves.toBe(false);
    expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
    view.changeScope("connection-a:0");
    let staleAnswer!: Promise<boolean>;
    act(() => { staleAnswer = oldConfirm("过期删除？"); });
    await expect(staleAnswer).resolves.toBe(false);
    expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();

    const currentAnswer = view.request("删除当前键？");
    fireEvent.click(screen.getByRole("button", { name: "确认删除" }));
    await expect(currentAnswer).resolves.toBe(true);
  });

  it("卸载时取消待定确认，并拒绝卸载后的确认请求", async () => {
    const view = renderConfirmation();
    const confirm = view.confirm;
    const answer = view.request();
    expect(screen.getByRole("alertdialog")).toBeInTheDocument();
    view.unmount();
    await expect(answer).resolves.toBe(false);
    await expect(confirm("已卸载的请求？")).resolves.toBe(false);
    expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
  });
});
