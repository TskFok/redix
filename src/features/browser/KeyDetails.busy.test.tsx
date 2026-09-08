import { useState } from "react";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { KeyValue } from "../../lib/types";
import KeyDetails from "./KeyDetails";

const { invokeMock } = vi.hoisted(() => ({ invokeMock: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));

const stream: KeyValue = { key: "events", key_type: "stream", ttl_ms: -1, value: { Stream: { entries: [] } } };
const pendingEntry = { id: "1-0", consumer: "worker", idle_ms: 100, deliveries: 1 };
const group = { name: "workers", consumers: 1, pending: 1, last_delivered_id: "2-0" };

function readResult(command: string): unknown {
  switch (command) {
    case "get_stream_entries": return { entries: [{ id: "1-0", fields: [{ field: "event", value: "ready" }] }], next_cursor: null, has_more: false };
    case "get_stream_consumer_groups": return [group];
    case "get_stream_consumers": return [{ name: "worker", pending: 1, idle_ms: 100 }];
    case "get_stream_pending_entries": return [pendingEntry];
    case "get_stream_pending_page": return { entries: [pendingEntry], next_cursor: null, has_more: false };
    default: throw new Error(`未设置的 IPC：${command}`);
  }
}

function deferred() {
  let resolve!: (value?: unknown) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<unknown>((success, failure) => { resolve = success; reject = failure; });
  return { promise, resolve, reject };
}

function Parent({ onGlobalAction }: { onGlobalAction: () => void }) {
  const [busy, setBusy] = useState(false);
  return <>
    <button disabled={busy} onClick={onGlobalAction}>全局切换数据库</button>
    <KeyDetails connectionId="local" detail={stream} loading={false}
      onDetailChange={() => undefined} onDeleted={() => undefined} onBusyChange={setBusy} />
  </>;
}

async function renderGroups(onGlobalAction = vi.fn()) {
  render(<Parent onGlobalAction={onGlobalAction} />);
  await screen.findByRole("checkbox", { name: "选择消息 1-0" });
  fireEvent.click(screen.getByRole("tab", { name: "消费者组" }));
  await screen.findByRole("checkbox", { name: "选择 Pending 1-0" });
  await waitFor(() => expect(screen.getByRole("button", { name: "全局切换数据库" })).toBeEnabled());
}

async function startGroupWrite(kind: string) {
  if (kind === "create") {
    fireEvent.change(screen.getByLabelText("Consumer Group 名称"), { target: { value: "next" } });
    fireEvent.click(screen.getByRole("button", { name: "创建 Group" }));
  } else if (kind === "delete-group" || kind === "delete-consumer") {
    fireEvent.click(screen.getByRole("button", { name: kind === "delete-group" ? "删除 Group" : "删除消费者 worker" }));
    fireEvent.click(within(screen.getByRole("alertdialog")).getByRole("button", { name: "确认删除" }));
  } else if (kind === "ack" || kind === "claim") {
    fireEvent.click(screen.getByRole("checkbox", { name: "选择 Pending 1-0" }));
    if (kind === "ack") fireEvent.click(screen.getByRole("button", { name: "确认选中" }));
    else {
      fireEvent.change(screen.getByLabelText("目标消费者"), { target: { value: "next" } });
      fireEvent.click(screen.getByRole("button", { name: "转移选中 Pending" }));
      fireEvent.click(within(screen.getByRole("alertdialog")).getByRole("button", { name: "确认转移" }));
    }
  } else {
    fireEvent.click(screen.getByRole("button", { name: "展开 Stream 高级操作" }));
    await screen.findByRole("checkbox", { name: "高级选择 Pending 1-0" });
    await waitFor(() => expect(screen.getByRole("button", { name: "更新 Group ID" })).toBeEnabled());
    if (kind === "setid") fireEvent.click(screen.getByRole("button", { name: "更新 Group ID" }));
    else {
      fireEvent.click(screen.getByRole("checkbox", { name: "高级选择 Pending 1-0" }));
      fireEvent.change(screen.getByLabelText("高级转移目标消费者"), { target: { value: "next" } });
      fireEvent.click(screen.getByRole("button", { name: "执行高级 Claim" }));
    }
  }
}

beforeEach(() => {
  vi.resetAllMocks();
  invokeMock.mockImplementation(async (command: string) => readResult(command));
});
afterEach(cleanup);

describe("Stream 写操作向键详情父层上报忙碌", () => {
  it.each([
    { kind: "create", command: "create_stream_consumer_group", result: undefined },
    { kind: "delete-group", command: "delete_stream_consumer_group", result: 1 },
    { kind: "delete-consumer", command: "delete_stream_consumer", result: 1 },
    { kind: "ack", command: "acknowledge_stream_pending_entries", result: 1 },
    { kind: "claim", command: "claim_stream_pending_entries", result: ["1-0"] },
    { kind: "setid", command: "update_stream_group_id", result: undefined },
    { kind: "advanced-claim", command: "claim_stream_pending_advanced", result: ["1-0"] },
  ])("$kind 完成前父层全局行为和键操作保持禁用", async ({ kind, command, result }) => {
    const writing = deferred();
    const globalAction = vi.fn();
    invokeMock.mockImplementation(async (name: string) => name === command ? writing.promise : readResult(name));
    await renderGroups(globalAction);
    await startGroupWrite(kind);
    await waitFor(() => expect(invokeMock.mock.calls.some(([name]) => name === command)).toBe(true));

    const switchButton = screen.getByRole("button", { name: "全局切换数据库" });
    await waitFor(() => expect(switchButton).toBeDisabled());
    fireEvent.click(switchButton);
    expect(globalAction).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("tab", { name: "键操作" }));
    expect(screen.getByRole("button", { name: "设置 TTL" })).toBeDisabled();

    await act(async () => { writing.resolve(result); });
    await waitFor(() => expect(switchButton).toBeEnabled());
    expect(screen.getByRole("button", { name: "设置 TTL" })).toBeEnabled();
    fireEvent.click(switchButton);
    expect(globalAction).toHaveBeenCalledOnce();
  });

  it("消费者组写操作失败后释放父层忙碌状态", async () => {
    const writing = deferred();
    invokeMock.mockImplementation(async (name: string) => name === "create_stream_consumer_group" ? writing.promise : readResult(name));
    await renderGroups();
    await startGroupWrite("create");
    expect(screen.getByRole("button", { name: "全局切换数据库" })).toBeDisabled();

    await act(async () => { writing.reject({ code: "COMMAND_FAILED" }); });
    expect(await screen.findByRole("alert")).toHaveTextContent("失败");
    expect(screen.getByRole("button", { name: "全局切换数据库" })).toBeEnabled();
  });

  it.each(["entries", "groups"])("%s 先完成时，另一面板仍在写入则不解除父层忙碌", async (first) => {
    const entries = deferred();
    const groups = deferred();
    invokeMock.mockImplementation(async (name: string) => name === "add_stream_entry" ? entries.promise
      : name === "create_stream_consumer_group" ? groups.promise : readResult(name));
    await renderGroups();
    fireEvent.click(screen.getByRole("tab", { name: "值" }));
    fireEvent.click(screen.getByRole("button", { name: "添加消息" }));
    fireEvent.click(within(screen.getByRole("alertdialog")).getByRole("button", { name: "确认添加" }));
    await waitFor(() => expect(invokeMock.mock.calls.some(([name]) => name === "add_stream_entry")).toBe(true));
    fireEvent.click(screen.getByRole("tab", { name: "消费者组" }));
    await startGroupWrite("create");
    const switchButton = screen.getByRole("button", { name: "全局切换数据库" });
    expect(switchButton).toBeDisabled();

    await act(async () => {
      if (first === "entries") entries.resolve("3-0");
      else groups.resolve();
    });
    expect(switchButton).toBeDisabled();

    await act(async () => {
      if (first === "entries") groups.resolve();
      else entries.resolve("3-0");
    });
    await waitFor(() => expect(switchButton).toBeEnabled());
  });
});
