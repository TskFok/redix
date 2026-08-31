import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import type { JsonPathValue, JsonValue } from "../../lib/types";
import JsonPathEditor from "./JsonPathEditor";

afterEach(cleanup);

function editor(value: JsonValue, onRead = vi.fn(), onMutate = vi.fn()) {
  return <JsonPathEditor value={value} busy={false} error={null} onRead={onRead} onMutate={onMutate} />;
}

describe("JSON 树定位", () => {
  it("折叠对象与数组并将节点的准确路径和值填回编辑器", () => {
    const mutations: unknown[] = [];
    const onMutate = vi.fn(async (mutation) => { mutations.push(mutation); });
    render(editor({ profile: { tags: ["redis", null] } }, vi.fn(), onMutate));
    expect(screen.queryByRole("button", { name: "定位 $.profile.tags" })).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "展开 $.profile" }));
    fireEvent.click(screen.getByRole("button", { name: "展开 $.profile.tags" }));
    fireEvent.click(screen.getByRole("button", { name: "定位 $.profile.tags[1]" }));
    expect(screen.getByLabelText("JSON Path")).toHaveValue("$.profile.tags[1]");
    expect(screen.getByLabelText("路径 JSON 值")).toHaveValue("null");
    fireEvent.click(screen.getByRole("button", { name: "保存路径" }));
    expect(mutations).toEqual([{ kind: "set", path: "$.profile.tags[1]", value: null }]);
    fireEvent.click(screen.getByRole("button", { name: "折叠 $.profile" }));
    expect(screen.queryByRole("button", { name: "定位 $.profile.tags[1]" })).not.toBeInTheDocument();
  });

  it.each([
    ["a.b", '$["a.b"]'],
    ['say"hi', '$["say\\"hi"]'],
    ["it's", '$["it\'s"]'],
    ["back\\slash", '$["back\\\\slash"]'],
    ["", '$[""]'],
    ["0", '$["0"]'],
    ["[*]", '$["[*]"]'],
    ["中文", '$["中文"]'],
    ["🚀", '$["🚀"]'],
    ["__proto__", "$.__proto__"],
  ])("特殊属性 %s 不会被当成其他节点", (property, path) => {
    const reads: string[] = [];
    const onRead = vi.fn(async (requested: string) => { reads.push(requested); });
    render(editor({ [property]: "exact", a: { b: "wrong" } }, onRead));
    fireEvent.click(screen.getByRole("button", { name: `定位 ${path}` }));
    expect(screen.getByLabelText("JSON Path")).toHaveValue(path);
    expect(screen.getByLabelText("路径 JSON 值")).toHaveValue('"exact"');
    fireEvent.click(screen.getByRole("button", { name: "读取路径" }));
    expect(reads).toEqual([path]);
  });

  it.each([
    ["换行", "line\nbreak"],
    ["C1 控制字符", "control\u0085"],
    ["孤立代理项", "\ud800"],
    ["超长 ASCII 属性", "x".repeat(513)],
    ["UTF-8 字节超限属性", "界".repeat(170)],
  ])("后端不能表达的节点仅可查看，不允许提交路径：%s", (_label, property) => {
    render(editor({ [property]: { child: 1 } }));
    const path = `$[${JSON.stringify(property)}]`;
    // 长纯字母属性使用点语法。
    const displayedPath = property.startsWith("xxx") ? `$.${property}` : path;
    fireEvent.click(screen.getByRole("button", { name: `定位 ${displayedPath}` }));
    expect(screen.getByLabelText("JSON Path")).toHaveValue(displayedPath);
    expect(screen.getByRole("status")).toHaveTextContent("仅可查看");
    for (const name of ["读取路径", "保存路径", "数组追加", "删除路径"]) {
      expect(screen.getByRole("button", { name })).toBeDisabled();
    }
    fireEvent.click(screen.getByRole("button", { name: `展开 ${displayedPath}` }));
    fireEvent.click(screen.getByRole("button", { name: `定位 ${displayedPath}.child` }));
    expect(screen.getByRole("button", { name: "保存路径" })).toBeDisabled();
    fireEvent.click(screen.getByRole("button", { name: "定位 $" }));
    expect(screen.getByRole("button", { name: "保存路径" })).toBeEnabled();
  });

  it("大数组分批展开并在达到总节点上限后停止渲染", () => {
    render(editor(Array.from({ length: 1500 }, (_, index) => index)));
    expect(screen.getAllByRole("treeitem").length).toBeLessThan(100);
    expect(screen.queryByRole("button", { name: "定位 $[1499]" })).not.toBeInTheDocument();
    for (let step = 0; step < 15; step += 1) {
      // Avoid recalculating accessible names for hundreds of unrelated buttons
      // on every page; this control has an explicit, stable aria-label.
      const more = screen.queryByLabelText("显示更多 $ 的子节点");
      if (!more) break;
      fireEvent.click(more);
    }
    expect(screen.getAllByRole("treeitem").length).toBeLessThanOrEqual(500);
    expect(screen.getByText(/已达到.*节点.*上限/)).toBeInTheDocument();
  });

  it("更换文档重置路径和展开状态，旧读取结果不能覆盖新节点", async () => {
    let resolve!: (value: JsonPathValue) => void;
    const onRead = vi.fn(() => new Promise<JsonPathValue>((done) => { resolve = done; }));
    const { rerender } = render(editor({ nested: { first: 1 }, second: 2 }, onRead));
    fireEvent.click(screen.getByRole("button", { name: "展开 $.nested" }));
    fireEvent.click(screen.getByRole("button", { name: "定位 $.nested.first" }));
    fireEvent.click(screen.getByRole("button", { name: "读取路径" }));
    fireEvent.click(screen.getByRole("button", { name: "定位 $.second" }));
    await act(async () => resolve({ key: "old", path: "$.nested.first", found: true, value: 1, ttl_ms: -1 }));
    expect(screen.queryByLabelText("路径读取结果")).not.toBeInTheDocument();
    rerender(editor({ nested: { first: 3 } }, onRead));
    expect(screen.getByLabelText("JSON Path")).toHaveValue("$");
    expect(screen.queryByRole("button", { name: "定位 $.nested.first" })).not.toBeInTheDocument();
  });
});
