import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import App from "./App";

describe("Redix 应用壳", () => {
  it("显示应用名称和默认工作区", () => {
    render(<App />);

    expect(screen.getByRole("heading", { name: "Redix" })).toBeInTheDocument();
    expect(screen.getByText("Browser")).toBeInTheDocument();
    expect(screen.getByText("Workbench")).toBeInTheDocument();
  });
});
