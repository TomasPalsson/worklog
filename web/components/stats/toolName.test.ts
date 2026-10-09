import { describe, expect, it } from "bun:test";
import { shortTool } from "./toolName";

describe("shortTool", () => {
  it("shortens known MCP names", () => {
    expect(shortTool("mcp__plugin_chrome-devtools-mcp_chrome-devtools__evaluate_script")).toBe("devtools·evaluate_script");
    expect(shortTool("mcp__plugin_Notion_notion__notion-create-file-upload")).toBe("notion·create-file-upload");
    expect(shortTool("mcp__claude_ai_Gmail__search_threads")).toBe("gmail·search_threads");
  });
  it("leaves other names and malformed MCP names alone", () => {
    for (const n of ["Bash", "Read", "mcp__", "mcp__nosplit", "mcp__srv__", ""]) expect(shortTool(n)).toBe(n);
  });
  it("keeps a tool that equals the server prefix", () => {
    expect(shortTool("mcp__srv__srv-")).toBe("srv·srv-");
  });
});
