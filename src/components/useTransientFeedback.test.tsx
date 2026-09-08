import { StrictMode } from "react";
import { act, cleanup, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { useTransientFeedback } from "./useTransientFeedback";

beforeEach(() => { vi.useFakeTimers(); });
afterEach(() => { cleanup(); vi.useRealTimers(); });

it.each(["已保存", "已导出"])("重复或更新为“%s”时从最新一次提示重新计时", (message) => {
  const { result, rerender } = renderHook(() => useTransientFeedback());
  act(() => { result.current[1]("已保存"); });
  act(() => { vi.advanceTimersByTime(2000); });
  act(() => { result.current[1](message); });
  act(() => { vi.advanceTimersByTime(1000); });
  expect(result.current[0]).toBe(message);
  rerender();
  act(() => { vi.advanceTimersByTime(2000); });
  expect(result.current[0]).toBeNull();
});

it("持久警告不会被之前成功提示的计时器清除，后续成功仍会自动消失", () => {
  const { result } = renderHook(() => useTransientFeedback());
  act(() => { result.current[1]("已保存"); });
  act(() => { vi.advanceTimersByTime(2000); });
  act(() => { result.current[1]("部分失败，请检查", null); });
  act(() => { vi.advanceTimersByTime(10_000); });
  expect(result.current[0]).toBe("部分失败，请检查");
  act(() => { result.current[1]("已重试成功"); });
  act(() => { vi.advanceTimersByTime(3000); });
  expect(result.current[0]).toBeNull();
});

it("主动清除和卸载时取消计时器，StrictMode 下仍正常清理", () => {
  const { result, unmount } = renderHook(() => useTransientFeedback(), { wrapper: StrictMode });
  act(() => { result.current[1]("已保存"); });
  expect(vi.getTimerCount()).toBe(1);
  act(() => { result.current[1](null); });
  expect(result.current[0]).toBeNull();
  expect(vi.getTimerCount()).toBe(0);
  act(() => { result.current[1]("已导出"); });
  unmount();
  expect(vi.getTimerCount()).toBe(0);
});
