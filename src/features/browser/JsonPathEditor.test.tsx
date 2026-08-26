import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import JsonPathEditor from "./JsonPathEditor";
import type { JsonMutationResult, JsonPathValue, JsonValue } from "../../lib/types";
import type { JsonPathMutation } from "./JsonPathEditor";

const rootValue: JsonValue = {
  name: "Alice",
  tags: ["redis"],
};

const mutationResult: JsonMutationResult = {
  key: "profile:1",
  path: "$.name",
  affected: 1,
  new_length: null,
  ttl_ms: 5000,
};

describe("JsonPathEditor", () => {
  afterEach(() => {
    cleanup();
  });

  it("读取路径调用 onRead 并展示返回的值、TTL 和 path", async () => {
    const pathValue: JsonPathValue = {
      key: "profile:1",
      path: "$.profile.name",
      value: "Alice",
      ttl_ms: 5000,
    };
    const onRead = vi.fn().mockResolvedValue(pathValue);

    render(
      <JsonPathEditor
        value={rootValue}
        busy={false}
        error={null}
        onRead={onRead}
        onMutate={vi.fn().mockResolvedValue(mutationResult)}
      />,
    );

    fireEvent.change(screen.getByLabelText("JSON Path"), {
      target: { value: "$.profile.name" },
    });
    fireEvent.click(screen.getByRole("button", { name: "读取路径" }));

    await waitFor(() => {
      expect(onRead).toHaveBeenCalledWith("$.profile.name");
    });
    expect(await screen.findByText("路径：$.profile.name")).toBeInTheDocument();
    expect(screen.getByText("TTL：5000 ms")).toBeInTheDocument();
    expect(screen.getByLabelText("路径读取结果")).toHaveTextContent('"Alice"');
  });

  it("拒绝非法 JSON、set 空值、append 非数组和 delete 空路径", async () => {
    const onRead = vi.fn().mockResolvedValue({
      key: "profile:1",
      path: "$",
      value: rootValue,
      ttl_ms: -1,
    } satisfies JsonPathValue);
    const onMutate = vi.fn().mockResolvedValue(mutationResult);

    render(
      <JsonPathEditor
        value={rootValue}
        busy={false}
        error={null}
        onRead={onRead}
        onMutate={onMutate}
      />,
    );

    fireEvent.change(screen.getByLabelText("路径 JSON 值"), {
      target: { value: '{"name":' },
    });
    fireEvent.click(screen.getByRole("button", { name: "保存路径" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("JSON 格式无效");
    expect(onMutate).not.toHaveBeenCalled();

    fireEvent.change(screen.getByLabelText("路径 JSON 值"), {
      target: { value: "   " },
    });
    fireEvent.click(screen.getByRole("button", { name: "保存路径" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("JSON 值不能为空");
    expect(onMutate).not.toHaveBeenCalled();

    fireEvent.change(screen.getByLabelText("路径 JSON 值"), {
      target: { value: '{"name":"Bob"}' },
    });
    fireEvent.click(screen.getByRole("button", { name: "数组追加" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("数组追加需要 JSON 数组");
    expect(onMutate).not.toHaveBeenCalled();

    fireEvent.change(screen.getByLabelText("JSON Path"), {
      target: { value: " " },
    });
    fireEvent.click(screen.getByRole("button", { name: "删除路径" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("JSON Path 不能为空");
    expect(onMutate).not.toHaveBeenCalled();
  });

  it("set、数组追加、delete 分别传递正确的 typed mutation", async () => {
    const mutations: JsonPathMutation[] = [];
    const onMutate = vi.fn(async (mutation: JsonPathMutation) => {
      mutations.push(mutation);
      return mutationResult;
    });

    render(
      <JsonPathEditor
        value={rootValue}
        busy={false}
        error={null}
        onRead={vi.fn()}
        onMutate={onMutate}
      />,
    );

    fireEvent.change(screen.getByLabelText("JSON Path"), {
      target: { value: "$.name" },
    });
    fireEvent.change(screen.getByLabelText("路径 JSON 值"), {
      target: { value: '"Bob"' },
    });
    fireEvent.click(screen.getByRole("button", { name: "保存路径" }));

    fireEvent.change(screen.getByLabelText("JSON Path"), {
      target: { value: "$.tags" },
    });
    fireEvent.change(screen.getByLabelText("路径 JSON 值"), {
      target: { value: '["redis","json"]' },
    });
    fireEvent.click(screen.getByRole("button", { name: "数组追加" }));

    fireEvent.change(screen.getByLabelText("JSON Path"), {
      target: { value: "$.obsolete" },
    });
    fireEvent.click(screen.getByRole("button", { name: "删除路径" }));

    await waitFor(() => {
      expect(onMutate).toHaveBeenCalledTimes(3);
    });
    expect(mutations).toEqual([
      { kind: "set", path: "$.name", value: "Bob" },
      { kind: "append", path: "$.tags", values: ["redis", "json"] },
      { kind: "delete", path: "$.obsolete" },
    ]);
  });

  it("允许 JSON null 作为 set 值，并允许数组追加 null 元素", async () => {
    const onMutate = vi.fn().mockResolvedValue(mutationResult);

    render(
      <JsonPathEditor
        value={rootValue}
        busy={false}
        error={null}
        onRead={vi.fn()}
        onMutate={onMutate}
      />,
    );

    fireEvent.change(screen.getByLabelText("JSON Path"), {
      target: { value: "$.optional" },
    });
    fireEvent.change(screen.getByLabelText("路径 JSON 值"), {
      target: { value: "null" },
    });
    fireEvent.click(screen.getByRole("button", { name: "保存路径" }));

    fireEvent.change(screen.getByLabelText("JSON Path"), {
      target: { value: "$.items" },
    });
    fireEvent.change(screen.getByLabelText("路径 JSON 值"), {
      target: { value: "[null]" },
    });
    fireEvent.click(screen.getByRole("button", { name: "数组追加" }));

    await waitFor(() => {
      expect(onMutate).toHaveBeenCalledTimes(2);
    });
    expect(onMutate).toHaveBeenNthCalledWith(1, {
      kind: "set",
      path: "$.optional",
      value: null,
    });
    expect(onMutate).toHaveBeenNthCalledWith(2, {
      kind: "append",
      path: "$.items",
      values: [null],
    });
  });

  it("父级传入的异步错误优先于本地 fallback 显示", async () => {
    const onRead = vi.fn().mockRejectedValue(new Error("read failed"));
    const { rerender } = render(
      <JsonPathEditor
        value={rootValue}
        busy={false}
        error={null}
        onRead={onRead}
        onMutate={vi.fn()}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: "读取路径" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("读取 JSON Path 失败");

    rerender(
      <JsonPathEditor
        value={rootValue}
        busy={false}
        error="未找到匹配的 JSON Path。"
        onRead={onRead}
        onMutate={vi.fn()}
      />,
    );

    expect(screen.getByRole("alert")).toHaveTextContent("未找到匹配的 JSON Path。");
  });

  it("busy 时禁用输入和按钮，异步错误显示在编辑器内", () => {
    render(
      <JsonPathEditor
        value={rootValue}
        busy={true}
        error="读取 JSON Path 失败，请稍后重试。"
        onRead={vi.fn()}
        onMutate={vi.fn()}
      />,
    );

    expect(screen.getByLabelText("JSON Path")).toBeDisabled();
    expect(screen.getByLabelText("路径 JSON 值")).toBeDisabled();
    expect(screen.getByRole("button", { name: "读取路径" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "保存路径" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "数组追加" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "删除路径" })).toBeDisabled();
    expect(screen.getByRole("alert")).toHaveTextContent("读取 JSON Path 失败");
  });
});
