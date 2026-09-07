import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { ConnectionTagsPanel, filterConnectionsByTag } from "./ConnectionTags";
import type { ConnectionProfile } from "../../lib/types";

const save = vi.hoisted(() => vi.fn());
vi.mock("../../lib/localProductsApi", () => ({ saveConnectionTags: save }));
afterEach(() => { cleanup(); vi.clearAllMocks(); });

it("标签过滤匹配键和值，并保留未标记筛选", () => {
  const profiles = [{id:"one",name:"one"},{id:"two",name:"two"}] as ConnectionProfile[];
  const tags = {one:[{key:"env",value:"prod"}]};
  expect(filterConnectionsByTag(profiles,tags,"prod").map(p=>p.id)).toEqual(["one"]);
  expect(filterConnectionsByTag(profiles,tags,"ENV").map(p=>p.id)).toEqual(["one"]);
  expect(filterConnectionsByTag(profiles,tags,"",true).map(p=>p.id)).toEqual(["two"]);
});

it("缺少标签的特殊连接ID不会读取对象原型属性", () => {
  const profiles = ["toString", "__proto__", "constructor"].map(id => ({id, name:id})) as ConnectionProfile[];
  expect(filterConnectionsByTag(profiles, {}, "prod")).toEqual([]);
  expect(filterConnectionsByTag(profiles, {}, "", true)).toEqual(profiles);
});

it("编辑、删除标签并保存后更新父状态，失败保留输入", async () => {
  const onSaved=vi.fn();
  save.mockResolvedValue([{key:"env",value:"prod"}]);
  render(<ConnectionTagsPanel connectionId="one" connectionName="本地" tags={[]} onSaved={onSaved} />);
  fireEvent.click(screen.getByRole("button",{name:"管理标签 本地"}));
  fireEvent.click(screen.getByRole("button",{name:"添加标签"}));
  fireEvent.change(screen.getByLabelText("标签键 1"),{target:{value:"env"}});
  fireEvent.change(screen.getByLabelText("标签值 1"),{target:{value:"prod"}});
  fireEvent.click(screen.getByRole("button",{name:"保存标签"}));
  await waitFor(()=>expect(onSaved).toHaveBeenCalledWith("one",[{key:"env",value:"prod"}]));
  fireEvent.click(screen.getByRole("button",{name:"管理标签 本地"}));
  fireEvent.click(screen.getByRole("button",{name:"添加标签"}));
  fireEvent.change(screen.getByLabelText("标签键 1"),{target:{value:"env"}});
  fireEvent.change(screen.getByLabelText("标签值 1"),{target:{value:"test"}});
  save.mockRejectedValue({message:"private error"});
  fireEvent.click(screen.getByRole("button",{name:"保存标签"}));
  expect(await screen.findByRole("alert")).not.toHaveTextContent("private error");
  expect(screen.getByLabelText("标签值 1")).toHaveValue("test");
});
