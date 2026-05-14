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
  | { kind: "text"; role: Role; text: string; cancelled?: boolean }
  | { kind: "thinking"; text: string; redacted?: boolean }
  | { kind: "image"; role: Role; mimeType: string; data: string; alt?: string }
  | { kind: "file"; role: Role; name: string; mimeType: string; size: number }
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

export type AttachmentKind = "image" | "pdf" | "text";

interface AttachmentBase {
  id: string;
  name: string;
  mimeType: string;
  /** byte length of the original file */
  size: number;
}

export interface ImageAttachment extends AttachmentBase {
  kind: "image";
  /** base64-encoded image bytes (no data: prefix) */
  data: string;
  /** ready-to-render data: URL, derived from mimeType + data */
  dataUrl: string;
}

export interface PdfAttachment extends AttachmentBase {
  kind: "pdf";
  /** base64-encoded PDF bytes */
  data: string;
}

export interface TextAttachment extends AttachmentBase {
  kind: "text";
  /** decoded UTF-8 text contents */
  text: string;
}

export type Attachment = ImageAttachment | PdfAttachment | TextAttachment;
