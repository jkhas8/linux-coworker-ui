// Convert raw stream-json events into UI display blocks.
//
// Claude Code emits events in roughly these shapes (truncated):
//   {type:"system", subtype:"init", ...}
//   {type:"assistant", message:{role:"assistant", content:[{type:"text",text:...},
//                                                          {type:"tool_use",id,name,input}]}}
//   {type:"user", message:{role:"user", content:[{type:"tool_result", tool_use_id, content, is_error}]}}
//   {type:"result", subtype:"success", ...}
//   {type:"stream_event", event:{type:"content_block_delta", delta:{type:"text_delta",text:...}}}
//   {type:"stderr", text:...}            (synthesized by our backend)
//   {type:"unparsed", text:...}          (synthesized by our backend)

import type { DisplayBlock } from "./types";

export function eventToBlocks(raw: any): DisplayBlock[] {
  if (!raw || typeof raw !== "object") return [];

  switch (raw.type) {
    case "assistant":
      return contentToBlocks("assistant", raw.message?.content);
    case "user":
      return contentToBlocks("user", raw.message?.content);
    case "system":
      if (raw.subtype === "init") {
        return [
          {
            kind: "system",
            text: `session ${raw.session_id ?? ""} ready · model ${raw.model ?? "?"}`,
          },
        ];
      }
      return [{ kind: "system", text: JSON.stringify(raw) }];
    case "result":
      return [
        {
          kind: "system",
          text:
            raw.subtype === "success"
              ? `done · ${raw.num_turns ?? "?"} turns · $${raw.total_cost_usd ?? 0}`
              : `result: ${raw.subtype ?? "?"}`,
        },
      ];
    case "stderr":
      return [{ kind: "error", text: raw.text ?? "" }];
    case "unparsed":
      return [{ kind: "error", text: `unparsed: ${raw.text}` }];
    case "stream_event":
    case "rate_limit_event":
      // Informational / streaming deltas — drop quietly.
      return [];
    default:
      // eslint-disable-next-line no-console
      console.warn("[stream] unknown top-level event", raw.type, raw);
      return [{ kind: "system", text: `[unhandled ${raw.type ?? "event"}]` }];
  }
}

function contentToBlocks(role: "user" | "assistant", content: any): DisplayBlock[] {
  if (!Array.isArray(content)) return [];
  const out: DisplayBlock[] = [];
  for (const c of content) {
    if (!c || typeof c !== "object") continue;
    switch (c.type) {
      case "text":
        if (c.text && c.text.trim()) out.push({ kind: "text", role, text: c.text });
        break;
      case "thinking":
        if (c.thinking && c.thinking.trim())
          out.push({ kind: "thinking", text: c.thinking });
        break;
      case "redacted_thinking":
        out.push({ kind: "thinking", text: "(redacted)", redacted: true });
        break;
      // Local tools (Bash/Read/Edit/our MCP server, etc.)
      case "tool_use":
        if (c.name === "AskUserQuestion") {
          // Render as an interactive form rather than a static tool card.
          out.push({
            kind: "ask_question",
            id: c.id,
            questions: Array.isArray(c.input?.questions) ? c.input.questions : [],
          });
        } else {
          out.push({
            kind: "tool_call",
            call: { id: c.id, name: c.name, input: c.input },
          });
        }
        break;
      case "tool_result":
        out.push({
          kind: "tool_result",
          result: {
            tool_use_id: c.tool_use_id,
            content: c.content,
            is_error: c.is_error,
          },
        });
        break;
      // Server-side tools (Anthropic-hosted web search etc.)
      case "server_tool_use":
        out.push({
          kind: "tool_call",
          call: { id: c.id, name: c.name ?? "server_tool", input: c.input },
        });
        break;
      case "web_search_tool_result":
        out.push({
          kind: "tool_result",
          result: {
            tool_use_id: c.tool_use_id,
            content: c.content,
            is_error: !!c.error,
          },
        });
        break;
      case "image":
        if (c.source?.data && c.source?.media_type) {
          out.push({
            kind: "image",
            role,
            mimeType: c.source.media_type,
            data: c.source.data,
          });
        }
        break;
      case "document":
        // The model never re-emits user-attached documents in practice, but if
        // a future content block shows up we render a compact file chip.
        out.push({
          kind: "file",
          role,
          name: c.title ?? c.source?.url ?? "document",
          mimeType: c.source?.media_type ?? "application/octet-stream",
          size: 0,
        });
        break;
      default: {
        // Unknown content block — log so we can add a handler, and render
        // a compact debug row so the user sees that *something* was here.
        // eslint-disable-next-line no-console
        console.warn("[stream] unknown content type", c.type, c);
        out.push({
          kind: "system",
          text: `[unhandled ${c.type ?? "block"}]`,
        });
      }
    }
  }
  return out;
}
