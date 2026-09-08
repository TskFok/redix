import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import StreamAdvancedPanel from "./StreamAdvancedPanel";

const api = vi.hoisted(() => ({ getStreamPendingPage: vi.fn(), claimStreamPendingAdvanced: vi.fn(), updateStreamGroupId: vi.fn() }));
vi.mock("./streamAdvancedApi", () => api);
const entry = (id: string) => ({id, consumer:"old", idle_ms:100, deliveries:2});
const props = {connectionId:"local",streamKey:"events",group:"workers",lastDeliveredId:"2-0",onChanged:vi.fn()};
beforeEach(() => {
  vi.clearAllMocks();
  api.getStreamPendingPage.mockResolvedValue({entries:[entry("1-0")],next_cursor:null,has_more:false});
  api.updateStreamGroupId.mockResolvedValue(undefined);
  api.claimStreamPendingAdvanced.mockResolvedValue(["1-0"]);
});
afterEach(() => { cleanup(); vi.useRealTimers(); });

it("展开后范围读取、下一页使用排除游标，切换范围重置分页", async () => {
  api.getStreamPendingPage.mockResolvedValueOnce({entries:[entry("1-0")],next_cursor:"1-0",has_more:true});
  render(<StreamAdvancedPanel {...props}/>);
  expect(api.getStreamPendingPage).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("button",{name:"展开 Stream 高级操作"}));
  await screen.findByRole("checkbox",{name:"高级选择 Pending 1-0"});
  expect(api.getStreamPendingPage).toHaveBeenCalledWith({connection_id:"local",key:"events",group:"workers",consumer:null,start:"-",end:"+",cursor:null,count:100});
  api.getStreamPendingPage.mockResolvedValueOnce({entries:[entry("2-0")],next_cursor:null,has_more:false});
  fireEvent.click(screen.getByRole("button",{name:"Pending 下一页"}));
  await screen.findByRole("checkbox",{name:"高级选择 Pending 2-0"});
  expect(api.getStreamPendingPage.mock.lastCall?.[0].cursor).toBe("1-0");
  fireEvent.change(screen.getByLabelText("Pending 起始 ID"),{target:{value:"3-0"}});
  fireEvent.change(screen.getByLabelText("Pending 消费者过滤"),{target:{value:"worker"}});
  fireEvent.click(screen.getByRole("button",{name:"应用 Pending 范围"}));
  await waitFor(()=>expect(api.getStreamPendingPage.mock.lastCall?.[0]).toMatchObject({start:"3-0",consumer:"worker",cursor:null}));
});

it("SETID仅提交目标Group并刷新，非法ID不发送", async () => {
  render(<StreamAdvancedPanel {...props}/>);
  fireEvent.click(screen.getByRole("button",{name:"展开 Stream 高级操作"}));
  await screen.findByRole("checkbox",{name:"高级选择 Pending 1-0"});
  fireEvent.change(screen.getByLabelText("Group 最后投递 ID"),{target:{value:"*"}});
  fireEvent.click(screen.getByRole("button",{name:"更新 Group ID"}));
  expect(api.updateStreamGroupId).not.toHaveBeenCalled();
  fireEvent.change(screen.getByLabelText("Group 最后投递 ID"),{target:{value:"0"}});
  fireEvent.click(screen.getByRole("button",{name:"更新 Group ID"}));
  await waitFor(()=>expect(api.updateStreamGroupId).toHaveBeenCalledWith({connection_id:"local",key:"events",group:"workers",last_delivered_id:"0"}));
  expect(props.onChanged).toHaveBeenCalled();
});

it("高级Claim传递TIME、RETRYCOUNT、FORCE，展示影响并只返回成功ID", async () => {
  render(<StreamAdvancedPanel {...props}/>);
  fireEvent.click(screen.getByRole("button",{name:"展开 Stream 高级操作"}));
  await screen.findByRole("checkbox",{name:"高级选择 Pending 1-0"});
  fireEvent.click(screen.getByRole("checkbox",{name:"高级选择 Pending 1-0"}));
  fireEvent.change(screen.getByLabelText("高级转移目标消费者"),{target:{value:"new"}});
  fireEvent.change(screen.getByLabelText("投递时间设置"),{target:{value:"time"}});
  fireEvent.change(screen.getByLabelText("TIME Unix 毫秒"),{target:{value:"1234"}});
  fireEvent.change(screen.getByLabelText("RETRYCOUNT 投递次数"),{target:{value:"7"}});
  fireEvent.click(screen.getByRole("checkbox",{name:"FORCE 创建 Pending 记录"}));
  expect(screen.getByText(/即使不在 Pending/)).toBeInTheDocument();
  fireEvent.click(screen.getByRole("button",{name:"执行高级 Claim"}));
  await waitFor(()=>expect(api.claimStreamPendingAdvanced).toHaveBeenCalledWith({connection_id:"local",key:"events",group:"workers",consumer:"new",min_idle_ms:0,entries:["1-0"],idle_ms:null,time_ms:1234,retry_count:7,force:true}));
  expect(await screen.findByText(/已转移 1 \/ 1 条/)).toBeInTheDocument();
});

it("完整与部分 Claim 提示均自动消失，部分结果显示为错误", async () => {
  render(<StreamAdvancedPanel {...props}/>);
  fireEvent.click(screen.getByRole("button",{name:"展开 Stream 高级操作"}));
  await screen.findByRole("checkbox",{name:"高级选择 Pending 1-0"});
  vi.useFakeTimers();
  fireEvent.click(screen.getByRole("checkbox",{name:"高级选择 Pending 1-0"}));
  fireEvent.change(screen.getByLabelText("高级转移目标消费者"),{target:{value:"new"}});
  await act(async () => { fireEvent.click(screen.getByRole("button",{name:"执行高级 Claim"})); });
  expect(screen.getByRole("status")).toHaveTextContent("已转移 1 / 1 条");
  act(() => { vi.advanceTimersByTime(3000); });
  expect(screen.queryByRole("status")).not.toBeInTheDocument();

  api.claimStreamPendingAdvanced.mockResolvedValueOnce([]);
  fireEvent.change(screen.getByLabelText("补充消息 ID"),{target:{value:"1-0"}});
  await act(async () => { fireEvent.click(screen.getByRole("button",{name:"执行高级 Claim"})); });
  expect(screen.getByRole("alert")).toHaveTextContent("已转移 0 / 1 条");
  act(() => { vi.advanceTimersByTime(3000); });
  expect(screen.queryByRole("alert")).not.toBeInTheDocument();
});

it("切换Group后丢弃旧请求，错误不泄露服务端消息", async () => {
  let resolve!: (value: unknown)=>void;
  api.getStreamPendingPage.mockReturnValueOnce(new Promise(r=>{resolve=r;}));
  const view=render(<StreamAdvancedPanel {...props}/>);
  fireEvent.click(screen.getByRole("button",{name:"展开 Stream 高级操作"}));
  api.getStreamPendingPage.mockRejectedValue({message:"secret"});
  view.rerender(<StreamAdvancedPanel {...props} group="new-group"/>);
  resolve({entries:[entry("old-0")],next_cursor:null,has_more:false});
  await waitFor(()=>expect(screen.queryByText("old-0")).not.toBeInTheDocument());
  fireEvent.click(screen.getByRole("button",{name:"展开 Stream 高级操作"}));
  expect(await screen.findByRole("alert")).not.toHaveTextContent("secret");
});
