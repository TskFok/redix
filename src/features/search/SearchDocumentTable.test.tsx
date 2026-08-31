import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, it } from "vitest";
import { SearchDocumentTable } from "./SearchDocumentTable";

afterEach(cleanup);

it("限制动态列数量但保留额外字段，字段名和内容以文本显示", () => {
  render(<SearchDocumentTable documents={[{ key: "doc", key_type: "hash", fields: [
    ...Array.from({ length: 30 }, (_, index) => ({ name: `field${index}`, value: index })),
    { name: "__proto__", value: "<script>alert(1)</script>" },
  ] }]} />);
  expect(screen.getAllByRole("columnheader")).toHaveLength(28);
  expect(screen.queryByRole("columnheader", { name: "field29" })).not.toBeInTheDocument();
  expect(screen.getByText("7 个其他字段")).toBeInTheDocument();
  fireEvent.click(screen.getByText("7 个其他字段"));
  expect(screen.getByText("field29")).toBeInTheDocument();
  expect(screen.getByText("__proto__")).toBeInTheDocument();
  expect(screen.getByText("<script>alert(1)</script>")).toBeInTheDocument();
  expect(document.querySelector("script")).toBeNull();
});

it("长字段按需展开，空文档与未返回的字段不冒充 null", () => {
  render(<SearchDocumentTable documents={[
    { key: "long", key_type: "hash", fields: [{ name: "body", value: `${"x".repeat(2400)}TAIL` }] },
    { key: "empty", key_type: "hash", fields: [] },
    { key: "gone", key_type: "none", fields: null },
  ]} />);
  expect(screen.queryByText(/TAIL/)).not.toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "展开完整字段" }));
  expect(screen.getByText(/TAIL/)).toBeInTheDocument();
  expect(screen.getByText("无字段")).toBeInTheDocument();
  expect(screen.getByText("文档已过期或内容不可用")).toBeInTheDocument();
  expect(screen.getAllByLabelText("字段不存在")).toHaveLength(2);
});
