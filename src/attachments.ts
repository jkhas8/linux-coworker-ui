// Helpers for turning a `File`/`Blob` into an `Attachment`. Supports three
// flavors: images (sent as image blocks), PDFs (sent as document blocks), and
// text-ish files (decoded UTF-8, inlined into the user message as text).
// Spreadsheets (xlsx/xls/ods/csv) are parsed client-side and inlined as CSV.

import * as XLSX from "xlsx";
import type { Attachment, AttachmentKind } from "./types";

const IMAGE_MIME = new Set([
  "image/png",
  "image/jpeg",
  "image/gif",
  "image/webp",
]);

// Claude API limits — leave a little headroom under the documented caps.
const MAX_IMAGE_BYTES = 8 * 1024 * 1024; // ~5 MB API limit
const MAX_PDF_BYTES = 32 * 1024 * 1024; // 32 MB API limit
const MAX_TEXT_BYTES = 1 * 1024 * 1024; // 1 MB decoded — guards token blowup
const MAX_SPREADSHEET_BYTES = 16 * 1024 * 1024; // raw xlsx can be heavy; parsed text gets re-checked

// Spreadsheet MIME types we know how to parse with SheetJS.
const SPREADSHEET_MIME = new Set([
  "application/vnd.ms-excel",
  "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
  "application/vnd.oasis.opendocument.spreadsheet",
]);
const SPREADSHEET_EXTENSIONS = new Set(["xlsx", "xls", "xlsm", "xlsb", "ods", "fods"]);

// MIME types that aren't text/* but are reliably UTF-8 text.
const TEXT_LIKE_MIME = new Set([
  "application/json",
  "application/ld+json",
  "application/xml",
  "application/javascript",
  "application/typescript",
  "application/x-sh",
  "application/x-yaml",
  "application/yaml",
  "application/toml",
  "application/x-toml",
  "application/sql",
  "application/x-httpd-php",
]);

// Extension fallback for files where the browser left `file.type` empty
// (common on Linux for many code/config files).
const TEXT_EXTENSIONS = new Set([
  "txt", "md", "markdown", "rst", "log", "csv", "tsv",
  "json", "jsonl", "json5", "ndjson",
  "yaml", "yml", "toml", "ini", "conf", "cfg", "env",
  "xml", "html", "htm", "svg", "css", "scss", "sass", "less",
  "js", "jsx", "ts", "tsx", "mjs", "cjs",
  "py", "rb", "go", "rs", "java", "kt", "kts", "swift",
  "c", "cc", "cpp", "cxx", "h", "hh", "hpp", "hxx",
  "cs", "php", "sh", "bash", "zsh", "fish", "ps1",
  "sql", "graphql", "gql", "proto",
  "lua", "r", "scala", "clj", "cljs", "ex", "exs", "erl",
  "hs", "ml", "fs", "fsx", "dart", "nim", "zig", "v", "vala",
  "vue", "svelte", "astro", "tsx",
  "lock", "gradle", "groovy", "make", "mk", "cmake",
  "dockerfile", "containerfile", "tf", "tfvars", "hcl",
]);

export interface AttachmentError {
  reason: string;
}

export async function fileToAttachment(
  file: File | Blob,
): Promise<Attachment | AttachmentError> {
  const name = (file as File).name ?? "attachment";
  const mime = file.type || guessMimeFromName(name) || "application/octet-stream";
  const kind = classify(mime, name);

  if (kind === "spreadsheet") {
    if (file.size > MAX_SPREADSHEET_BYTES) {
      return { reason: tooLargeMsg("Spreadsheet", file.size, MAX_SPREADSHEET_BYTES) };
    }
    try {
      const csv = await spreadsheetToCsv(file);
      if (csv.length > MAX_TEXT_BYTES) {
        return {
          reason: `Spreadsheet has too much content (${(csv.length / 1024 / 1024).toFixed(1)} MB of text after conversion).`,
        };
      }
      return {
        kind: "text",
        id: crypto.randomUUID(),
        name,
        mimeType: "text/csv",
        text: csv,
        size: file.size,
      };
    } catch (e) {
      return { reason: `Could not parse spreadsheet ${name}: ${(e as Error).message}` };
    }
  }

  if (kind === "image") {
    if (!IMAGE_MIME.has(mime)) {
      return { reason: `Unsupported image type: ${mime}` };
    }
    if (file.size > MAX_IMAGE_BYTES) {
      return { reason: tooLargeMsg("Image", file.size, MAX_IMAGE_BYTES) };
    }
    const data = await fileToBase64(file);
    return {
      kind: "image",
      id: crypto.randomUUID(),
      name,
      mimeType: mime,
      data,
      dataUrl: `data:${mime};base64,${data}`,
      size: file.size,
    };
  }

  if (kind === "pdf") {
    if (file.size > MAX_PDF_BYTES) {
      return { reason: tooLargeMsg("PDF", file.size, MAX_PDF_BYTES) };
    }
    const data = await fileToBase64(file);
    return {
      kind: "pdf",
      id: crypto.randomUUID(),
      name,
      mimeType: "application/pdf",
      data,
      size: file.size,
    };
  }

  if (kind === "text") {
    if (file.size > MAX_TEXT_BYTES) {
      return { reason: tooLargeMsg("Text file", file.size, MAX_TEXT_BYTES) };
    }
    try {
      const text = await readAsUtf8(file);
      return {
        kind: "text",
        id: crypto.randomUUID(),
        name,
        mimeType: mime.startsWith("text/") ? mime : "text/plain",
        text,
        size: file.size,
      };
    } catch (e) {
      return { reason: `Could not decode ${name} as UTF-8 text.` };
    }
  }

  return {
    reason: `Unsupported file: ${name} (${mime || "unknown type"}). Supported: images, PDF, text/code, CSV, and spreadsheets (xlsx/xls/ods).`,
  };
}

type Classification = AttachmentKind | "spreadsheet" | "unknown";

function classify(mime: string, name: string): Classification {
  if (mime.startsWith("image/")) return "image";
  if (mime === "application/pdf") return "pdf";
  if (SPREADSHEET_MIME.has(mime)) return "spreadsheet";
  const ext = extOf(name);
  if (ext && SPREADSHEET_EXTENSIONS.has(ext)) return "spreadsheet";
  if (mime.startsWith("text/")) return "text";
  if (TEXT_LIKE_MIME.has(mime)) return "text";
  if (ext === "pdf") return "pdf";
  if (ext && TEXT_EXTENSIONS.has(ext)) return "text";
  return "unknown";
}

async function spreadsheetToCsv(file: File | Blob): Promise<string> {
  const buf = await file.arrayBuffer();
  const wb = XLSX.read(buf, { type: "array" });
  const parts: string[] = [];
  for (const sheetName of wb.SheetNames) {
    const ws = wb.Sheets[sheetName];
    if (!ws) continue;
    const csv = XLSX.utils.sheet_to_csv(ws, { blankrows: false });
    if (!csv.trim()) continue;
    parts.push(`--- Sheet: ${sheetName} ---\n${csv.trimEnd()}`);
  }
  return parts.join("\n\n");
}

function extOf(name: string): string | null {
  const i = name.lastIndexOf(".");
  if (i < 0 || i === name.length - 1) return null;
  return name.slice(i + 1).toLowerCase();
}

function guessMimeFromName(name: string): string | null {
  const ext = extOf(name);
  if (!ext) return null;
  if (ext === "pdf") return "application/pdf";
  if (ext === "png") return "image/png";
  if (ext === "jpg" || ext === "jpeg") return "image/jpeg";
  if (ext === "gif") return "image/gif";
  if (ext === "webp") return "image/webp";
  if (ext === "json") return "application/json";
  if (ext === "md" || ext === "markdown") return "text/markdown";
  if (ext === "csv") return "text/csv";
  if (TEXT_EXTENSIONS.has(ext)) return "text/plain";
  return null;
}

async function fileToBase64(file: File | Blob): Promise<string> {
  const buf = new Uint8Array(await file.arrayBuffer());
  return bytesToBase64(buf);
}

async function readAsUtf8(file: File | Blob): Promise<string> {
  const buf = await file.arrayBuffer();
  // `fatal: true` rejects invalid UTF-8 so we don't silently send mojibake
  // for binary files that snuck past the mime/extension classifier.
  return new TextDecoder("utf-8", { fatal: true }).decode(buf);
}

function bytesToBase64(bytes: Uint8Array): string {
  // Chunk to avoid stack overflow on large arrays.
  let s = "";
  const CHUNK = 0x8000;
  for (let i = 0; i < bytes.length; i += CHUNK) {
    s += String.fromCharCode(...bytes.subarray(i, i + CHUNK));
  }
  return btoa(s);
}

function tooLargeMsg(label: string, size: number, max: number): string {
  return `${label} too large (${(size / 1024 / 1024).toFixed(1)} MB); max ${max / 1024 / 1024} MB.`;
}

export function filesFromDataTransfer(dt: DataTransfer | null): (File | Blob)[] {
  if (!dt) return [];
  const out: (File | Blob)[] = [];
  if (dt.items && dt.items.length) {
    for (const item of dt.items) {
      if (item.kind === "file") {
        const f = item.getAsFile();
        if (f) out.push(f);
      }
    }
    if (out.length) return out;
  }
  if (dt.files && dt.files.length) {
    for (const f of dt.files) out.push(f);
  }
  return out;
}

// Kept for backwards-compat with the paste handler, which only ever cares
// about images (clipboard contents).
export function imagesFromDataTransfer(
  dt: DataTransfer | null,
): (File | Blob)[] {
  return filesFromDataTransfer(dt).filter((f) =>
    (f.type || "").startsWith("image/"),
  );
}
