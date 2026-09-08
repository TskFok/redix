import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import QueryPackage from "./QueryPackage";
const api=vi.hoisted(()=>({importQueryPackage:vi.fn(),exportQueryPackage:vi.fn()}));
vi.mock("../../lib/localProductsApi",()=>api);
afterEach(()=>{cleanup();vi.useRealTimers();vi.restoreAllMocks();vi.clearAllMocks();vi.unstubAllGlobals();});

it.each(["导入", "导出"])("查询包%s成功提示自动消失", async (operation) => {
  vi.useFakeTimers();
  api.importQueryPackage.mockResolvedValue([]);
  api.exportQueryPackage.mockResolvedValue({format:"redix-query-library",version:1,items:[]});
  vi.spyOn(HTMLAnchorElement.prototype, "click").mockImplementation(() => {});
  render(<QueryPackage onImported={() => {}} />);
  await act(async () => {
    if (operation === "导出") fireEvent.click(screen.getByRole("button", { name: "导出查询包" }));
    else {
      const file = new File(["{}"], "queries.json", { type: "application/json" });
      Object.defineProperty(file, "text", { value: async () => "{}" });
      fireEvent.change(screen.getByLabelText("导入查询包文件"), { target: { files: [file] } });
    }
  });
  expect(screen.getByRole("region", { name: "操作提示" })).toContainElement(screen.getByRole("status"));
  expect(screen.getByRole("status")).toHaveTextContent(`已${operation} 0 条查询`);
  act(() => { vi.advanceTimersByTime(3000); });
  expect(screen.queryByRole("status")).not.toBeInTheDocument();
});

it("导入成功传回新增查询，错误文件显示临时 Toast 且不替换现有查询",async()=>{
  const onImported=vi.fn();
  api.importQueryPackage.mockResolvedValue([{id:"q",name:"读取",command:"GET x",tags:[],updated_at:1}]);
  render(<QueryPackage onImported={onImported}/>);
  const file=new File(["{}"],"queries.json",{type:"application/json"});
  Object.defineProperty(file,"text",{value:async()=>"{}"});
  fireEvent.change(screen.getByLabelText("导入查询包文件"),{target:{files:[file]}});
  await waitFor(()=>expect(onImported).toHaveBeenCalledTimes(1));
  expect(api.importQueryPackage).toHaveBeenCalledWith("{}");
  api.importQueryPackage.mockRejectedValue({message:"AUTH secret"});
  fireEvent.change(screen.getByLabelText("导入查询包文件"),{target:{files:[file]}});
  expect(await screen.findByRole("alert")).toHaveTextContent("导入失败");
  expect(screen.getByRole("region", { name: "操作提示" })).toContainElement(screen.getByRole("alert"));
  expect(screen.getByRole("alert")).not.toHaveTextContent("secret");
  expect(onImported).toHaveBeenCalledTimes(1);
});

it("导出调用后端验证并下载版本化查询包",async()=>{
  api.exportQueryPackage.mockResolvedValue({format:"redix-query-library",version:1,items:[]});
  const click=vi.spyOn(HTMLAnchorElement.prototype,"click").mockImplementation(()=>{});
  render(<QueryPackage onImported={vi.fn()}/>);
  fireEvent.click(screen.getByRole("button",{name:"导出查询包"}));
  await waitFor(()=>expect(click).toHaveBeenCalledTimes(1));
  expect(await screen.findByRole("status")).toHaveTextContent("已导出 0 条查询");
  click.mockRestore();
});
