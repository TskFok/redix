import { StrictMode } from "react";
import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import Toast from "./Toast";
import { useFeedbackState } from "./useFeedbackState";

beforeEach(() => { vi.useFakeTimers(); });
afterEach(() => { cleanup(); vi.useRealTimers(); });

it("成功与失败通知挂在页面外同一个通知区域，不占用操作区布局", () => {
  const view = render(<div><Toast kind="success" message="已导出 2 个键。" /><Toast kind="error" message="导出失败，请重试。" /></div>);
  const region = screen.getByRole("region", { name: "操作提示" });
  expect(view.container).not.toContainElement(region);
  expect(within(region).getByRole("status")).toHaveClass("toast-success");
  expect(within(region).getByRole("alert")).toHaveClass("toast-error");
  expect(screen.getAllByRole("region", { name: "操作提示" })).toHaveLength(1);
});

it.each(["success", "error"] as const)("%s 通知显示 3 秒后关闭并通知调用方", (kind) => {
  const onClose = vi.fn();
  render(<Toast kind={kind} message="操作反馈" onClose={onClose} />);
  act(() => { vi.advanceTimersByTime(2999); });
  expect(screen.getByText("操作反馈")).toBeInTheDocument();
  act(() => { vi.advanceTimersByTime(1); });
  expect(screen.queryByText("操作反馈")).not.toBeInTheDocument();
  expect(onClose).toHaveBeenCalledOnce();
  expect(screen.queryByRole("region", { name: "操作提示" })).not.toBeInTheDocument();
});

it("手动关闭只移除对应通知，取消其计时器且不抢焦点", () => {
  const onClose = vi.fn();
  render(<><button>导出</button><Toast kind="success" message="成功" onClose={onClose} /><Toast kind="error" message="失败" /></>);
  screen.getByRole("button", { name: "导出" }).focus();
  expect(screen.getByRole("button", { name: "导出" })).toHaveFocus();
  fireEvent.click(within(screen.getByRole("status")).getByRole("button", { name: "关闭提示" }));
  expect(screen.queryByRole("status")).not.toBeInTheDocument();
  expect(screen.getByRole("alert")).toHaveTextContent("失败");
  act(() => { vi.advanceTimersByTime(3000); });
  expect(onClose).toHaveBeenCalledOnce();
});

it("详细诊断可以保持显示，直到用户手动关闭", () => {
  render(<Toast kind="error" message="连接失败" details={["TCP 连接被拒绝"]} durationMs={null} />);
  act(() => vi.advanceTimersByTime(10000));
  expect(screen.getByRole("alert")).toHaveTextContent("TCP 连接被拒绝");
  fireEvent.click(screen.getByRole("button", { name: "关闭提示" }));
  expect(screen.queryByRole("alert")).not.toBeInTheDocument();
});

it("同文案新操作重置计时，普通重渲染不会延长通知", () => {
  const first = {};
  const second = {};
  const view = render(<Toast kind="success" message="已复制" resetKey={first} />);
  act(() => { vi.advanceTimersByTime(2000); });
  view.rerender(<Toast kind="success" message="已复制" resetKey={second} />);
  act(() => { vi.advanceTimersByTime(1000); });
  expect(screen.getByText("已复制")).toBeInTheDocument();
  view.rerender(<Toast kind="success" message="已复制" resetKey={second} />);
  act(() => { vi.advanceTimersByTime(2000); });
  expect(screen.queryByText("已复制")).not.toBeInTheDocument();
});

it("关闭后新操作可重新弹出，卸载清理通知区域和定时器", () => {
  const view = render(<StrictMode><Toast kind="error" message="操作失败" resetKey={1} details={["请检查连接"]} /></StrictMode>);
  fireEvent.click(screen.getByRole("button", { name: "关闭提示" }));
  view.rerender(<StrictMode><Toast kind="error" message="操作失败" resetKey={2} details={["请检查连接"]} /></StrictMode>);
  expect(screen.getByRole("alert")).toHaveTextContent("请检查连接");
  view.unmount();
  expect(vi.getTimerCount()).toBe(0);
  expect(screen.queryByRole("region", { name: "操作提示" })).not.toBeInTheDocument();
});

it("同一批次重复关闭只通知调用方一次", () => {
  const onClose = vi.fn();
  render(<Toast kind="error" message="失败" onClose={onClose} />);
  const button = screen.getByRole("button", { name: "关闭提示" });
  act(() => { button.click(); button.click(); });
  expect(onClose).toHaveBeenCalledOnce();
});

it("重复提交相同的错误会重启通知计时，关闭后也可以重新触发", () => {
  function Form() {
    const [error, setError, token] = useFeedbackState<string | null>(null);
    return <><button onClick={() => setError("名称不能为空")}>保存</button>
      {error && <Toast kind="error" message={error} resetKey={token} onClose={() => setError(null)} />}</>;
  }
  render(<Form />);
  fireEvent.click(screen.getByRole("button", { name: "保存" }));
  act(() => { vi.advanceTimersByTime(2000); });
  fireEvent.click(screen.getByRole("button", { name: "保存" }));
  act(() => { vi.advanceTimersByTime(1000); });
  expect(screen.getByRole("alert")).toHaveTextContent("名称不能为空");
  act(() => { vi.advanceTimersByTime(2000); });
  expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "保存" }));
  expect(screen.getByRole("alert")).toHaveTextContent("名称不能为空");
});
