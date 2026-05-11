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

export interface AskQuestionOption {
  label: string;
  description?: string;
}
export interface AskQuestion {
  question: string;
  header?: string;
  multiSelect?: boolean;
  options: AskQuestionOption[];
}

export type DisplayBlock =
  | { kind: "text"; role: Role; text: string }
  | { kind: "thinking"; text: string; redacted?: boolean }
  | { kind: "image"; role: Role; mimeType: string; data: string; alt?: string }
  | { kind: "tool_call"; call: ToolCall }
  | { kind: "tool_result"; result: ToolResult }
  | {
      kind: "ask_question";
      id: string;
      questions: AskQuestion[];
      submitted?: { lines: string[] };
    }
  | { kind: "system"; text: string }
  | { kind: "error"; text: string };

export interface Attachment {
  id: string;
  name?: string;
  mimeType: string;
  /** base64-encoded image bytes (no data: prefix) */
  data: string;
  /** ready-to-render data: URL, derived from mimeType + data */
  dataUrl: string;
  /** byte length of the original file */
  size: number;
}
