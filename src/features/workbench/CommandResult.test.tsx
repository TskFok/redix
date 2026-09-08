import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";

import CommandResult from "./CommandResult";

afterEach(() => {
  cleanup();
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

it("复制成功提示会自动消失", async () => {
  vi.stubGlobal("navigator", { clipboard: { writeText: vi.fn().mockResolvedValue(undefined) } });
  vi.useFakeTimers();
  render(<CommandResult result={{ kind: "string", value: "PONG" }} error={null} />);

  await act(async () => {
    fireEvent.click(screen.getByRole("button", { name: "复制结果" }));
  });

  expect(screen.getByRole("status")).toHaveTextContent("已复制");
  act(() => vi.advanceTimersByTime(3000));
  expect(screen.queryByText("已复制")).not.toBeInTheDocument();
});

it("复制失败显示错误 Toast 并自动消失", async () => {
  vi.stubGlobal("navigator", { clipboard: { writeText: vi.fn().mockRejectedValue(new Error("denied")) } });
  vi.useFakeTimers();
  render(<CommandResult result={{ kind: "string", value: "PONG" }} error={null} />);

  await act(async () => {
    fireEvent.click(screen.getByRole("button", { name: "复制结果" }));
  });

  expect(screen.getByRole("region", { name: "操作提示" })).toContainElement(screen.getByRole("alert"));
  expect(screen.getByRole("alert")).toHaveTextContent("复制失败，请手动复制结果。");
  act(() => vi.advanceTimersByTime(3000));
  expect(screen.queryByText("复制失败，请手动复制结果。")).not.toBeInTheDocument();
});
