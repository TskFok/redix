import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import KeyDetails from "./KeyDetails";

vi.mock("../../lib/tauri", () => ({ getKeySearchIndexes: vi.fn() }));
vi.mock("./CollectionDetails", () => ({ default: ({ kind, onBusyChange }: { kind: string; onBusyChange?: (busy: boolean) => void }) => <div><p>分页集合 {kind}</p><button onClick={() => onBusyChange?.(true)}>开始字段写入</button></div> }));
vi.mock("./StreamEntries", () => ({ default: () => <p>分页 Stream</p> }));
vi.mock("./StreamConsumerGroups", () => ({ default: () => <p>消费者组</p> }));
afterEach(cleanup);

it("空集合预览进入专用分页编辑器，不暴露整键覆盖保存", () => {
  render(<KeyDetails connectionId="one" detail={{ key: "large", key_type: "hash", ttl_ms: -1, value: { Hash: { fields: [] } } }} loading={false} onDetailChange={vi.fn()} onDeleted={vi.fn()} />);
  expect(screen.getByText("分页集合 hash")).toBeInTheDocument();
  expect(screen.queryByRole("button", { name: "保存" })).not.toBeInTheDocument();
  fireEvent.click(screen.getByRole("tab", { name: "键操作" }));
  expect(screen.getByRole("button", { name: "设置 TTL" })).toBeInTheDocument();
});

it("Stream 专用分页编辑与消费者组可通过标签访问", () => {
  render(<KeyDetails connectionId="one" detail={{ key: "events", key_type: "stream", ttl_ms: -1, value: { Stream: { entries: [] } } }} loading={false} onDetailChange={vi.fn()} onDeleted={vi.fn()} />);
  expect(screen.getByText("分页 Stream")).toBeVisible();
  expect(screen.getByText("消费者组", { selector: "p" })).not.toBeVisible();
  fireEvent.click(screen.getByRole("tab", { name: "消费者组" }));
  expect(screen.getByText("消费者组", { selector: "p" })).toBeVisible();
  expect(screen.getByText("分页 Stream")).not.toBeVisible();
  expect(screen.queryByRole("button", { name: "保存" })).not.toBeInTheDocument();
});

it("分页子编辑器写入期间禁用重命名、TTL 与整键删除", () => {
  render(<KeyDetails connectionId="one" detail={{ key: "large", key_type: "hash", ttl_ms: -1, value: { Hash: { fields: [] } } }} loading={false} onDetailChange={vi.fn()} onDeleted={vi.fn()} />);
  fireEvent.click(screen.getByRole("button", { name: "开始字段写入" }));
  fireEvent.click(screen.getByRole("tab", { name: "键操作" }));
  expect(screen.getByRole("button", { name: "重命名" })).toBeDisabled();
  expect(screen.getByRole("button", { name: "设置 TTL" })).toBeDisabled();
  expect(screen.getByRole("button", { name: "处理中…" })).toBeDisabled();
});
