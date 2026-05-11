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
      // Partial deltas — ignore for now; we render finalized messages.
      return [];
    default:
      return [{ kind: "system", text: JSON.stringify(raw) }];
  }
}

function contentToBlocks(role: "user" | "assistant", content: any): DisplayBlock[] {
  if (!Array.isArray(content)) return [];
  const out: DisplayBlock[] = [];
  for (const c of content) {
    if (!c || typeof c !== "object") continue;
    switch (c.type) {
      case "text":
        if (c.text) out.push({ kind: "text", role, text: c.text });
        break;
      case "tool_use":
        out.push({
          kind: "tool_call",
          call: { id: c.id, name: c.name, input: c.input },
        });
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
    }
  }
  return out;
}
