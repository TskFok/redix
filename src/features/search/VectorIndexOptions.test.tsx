import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { defaultSearchVectorConfig, validSearchVectorConfig, VectorIndexOptions } from "./VectorIndexOptions";
import { vectorSearchSupported } from "./searchState";

afterEach(cleanup);

describe("VECTOR 索引配置", () => {
  it("校验维度、可选参数范围和算法互斥选项", () => {
    const base = defaultSearchVectorConfig();
    expect(validSearchVectorConfig(base)).toBe(true);
    for (const dimension of [0, -1, 1.5, NaN, Infinity, 32769]) {
      expect(validSearchVectorConfig({ ...base, dimension })).toBe(false);
    }
    expect(validSearchVectorConfig({ ...base, initial_capacity: 0 })).toBe(false);
    expect(validSearchVectorConfig({ ...base, m: 16 })).toBe(false);
    expect(validSearchVectorConfig({ ...base, algorithm: "HNSW", block_size: 10 })).toBe(false);
    expect(validSearchVectorConfig({ ...base, algorithm: "HNSW", m: 513 })).toBe(false);
    expect(validSearchVectorConfig({ ...base, algorithm: "HNSW", ef_runtime: 4097 })).toBe(false);
    expect(validSearchVectorConfig({ ...base, algorithm: "HNSW", m: 16, ef_construction: 200, ef_runtime: 10 })).toBe(true);
  });

  it("切换算法清除不适用的高级参数并保留公共选项", () => {
    const onChange = vi.fn();
    const value = { ...defaultSearchVectorConfig(), initial_capacity: 100, block_size: 128 };
    render(<VectorIndexOptions value={value} index={0} disabled={false} onChange={onChange} />);
    fireEvent.change(screen.getByLabelText("字段 1 向量算法"), { target: { value: "HNSW" } });
    expect(onChange).toHaveBeenCalledWith({ ...defaultSearchVectorConfig(), algorithm: "HNSW", initial_capacity: 100 });
  });

  it.each([null, "", "bad", "2.2.0", "2.3.99", "2.4.bad"])("旧版或未知版本降级：%s", (version) => {
    expect(vectorSearchSupported(version)).toBe(false);
  });

  it.each(["2.4", "2.4.0", "2.10.0", "8.0.0"])("支持向量索引的版本：%s", (version) => {
    expect(vectorSearchSupported(version)).toBe(true);
  });
});
