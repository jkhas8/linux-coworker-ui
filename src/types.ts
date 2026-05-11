// Types for stream-json events produced by `claude --output-format stream-json`.
// We intentionally keep this loose: the schema evolves and we just want to
// render whatever comes through without crashing.

export type Role = "user" | "assistant" | "system" | "tool";

export interface AgentEvent {
  session_id: string;
  raw: any;
}

export interface ToolCall {
  id: string;
  name: string;
  input: any;
}

export interface ToolResult {
  tool_use_id: string;
  content: any;
  is_error?: boolean;
}

export type DisplayBlock =
  | { kind: "text"; role: Role; text: string }
  | { kind: "tool_call"; call: ToolCall }
  | { kind: "tool_result"; result: ToolResult }
  | { kind: "system"; text: string }
  | { kind: "error"; text: string };
