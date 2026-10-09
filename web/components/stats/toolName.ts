/** Display name for an MCP tool: "mcp__plugin_Notion_notion__notion-create-file-upload" -> "notion·create-file-upload". Others unchanged. */
export function shortTool(name: string): string {
  if (!name.startsWith("mcp__")) return name;
  const rest = name.slice(5);
  const cut = rest.lastIndexOf("__");
  if (cut <= 0 || cut + 2 >= rest.length) return name;
  const tokens = rest
    .slice(0, cut)
    .replace(/^(plugin_|claude_ai_)/, "")
    .split(/[_-]/)
    .filter((t) => t && t.toLowerCase() !== "mcp");
  const server = (tokens[tokens.length - 1] ?? "").toLowerCase();
  if (!server) return name;
  let tool = rest.slice(cut + 2);
  if (tool.toLowerCase().startsWith(`${server}-`) && tool.length > server.length + 1) tool = tool.slice(server.length + 1);
  return `${server}·${tool}`;
}
