import { createMemo } from "solid-js";
import { renderMarkdown } from "../markdown";

export function Markdown(props: { source: string }) {
  const html = createMemo(() => renderMarkdown(props.source));
  return <div class="md" innerHTML={html()} />;
}
