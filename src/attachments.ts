// Helpers for turning a `File`/`Blob` into an `Attachment`.

import type { Attachment } from "./types";

const MAX_BYTES = 8 * 1024 * 1024; // claude API limit is ~5 MB; leave headroom.
const ALLOWED_MIME = new Set([
  "image/png",
  "image/jpeg",
  "image/gif",
  "image/webp",
]);

export interface AttachmentError {
  reason: string;
}

export async function fileToAttachment(
  file: File | Blob,
): Promise<Attachment | AttachmentError> {
  const mime = file.type || "application/octet-stream";
  if (!ALLOWED_MIME.has(mime)) {
    return { reason: `Unsupported image type: ${mime}` };
  }
  if (file.size > MAX_BYTES) {
    return {
      reason: `Image too large (${(file.size / 1024 / 1024).toFixed(1)} MB); max ${MAX_BYTES / 1024 / 1024} MB.`,
    };
  }
  const buf = new Uint8Array(await file.arrayBuffer());
  const data = bytesToBase64(buf);
  return {
    id: crypto.randomUUID(),
    name: (file as File).name,
    mimeType: mime,
    data,
    dataUrl: `data:${mime};base64,${data}`,
    size: file.size,
  };
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

export function imagesFromDataTransfer(
  dt: DataTransfer | null,
): (File | Blob)[] {
  if (!dt) return [];
  const out: (File | Blob)[] = [];
  // Prefer items (lets us read images pasted from screenshot tools).
  if (dt.items && dt.items.length) {
    for (const item of dt.items) {
      if (item.kind === "file") {
        const f = item.getAsFile();
        if (f && f.type.startsWith("image/")) out.push(f);
      }
    }
    if (out.length) return out;
  }
  // Fallback to files.
  if (dt.files && dt.files.length) {
    for (const f of dt.files) {
      if (f.type.startsWith("image/")) out.push(f);
    }
  }
  return out;
}
