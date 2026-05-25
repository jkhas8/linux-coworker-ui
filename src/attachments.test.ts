import { describe, expect, it } from "vitest";
import { fileToAttachment } from "./attachments";

describe("fileToAttachment", () => {
  it("decodes a plain-text file as a text attachment", async () => {
    const file = new File(["hello world"], "note.txt", { type: "text/plain" });
    const result = await fileToAttachment(file);
    expect(result).toMatchObject({
      kind: "text",
      name: "note.txt",
      mimeType: "text/plain",
      text: "hello world",
      size: 11,
    });
  });

  it("classifies a JSON file by extension when MIME is missing", async () => {
    const file = new File(['{"a":1}'], "config.json", { type: "" });
    const result = await fileToAttachment(file);
    expect(result).toMatchObject({
      kind: "text",
      name: "config.json",
      text: '{"a":1}',
    });
  });

  it("rejects an unsupported binary file with a helpful reason", async () => {
    const file = new File([new Uint8Array([0xff, 0xd8])], "mystery.bin", {
      type: "application/octet-stream",
    });
    const result = await fileToAttachment(file);
    expect(result).toMatchObject({
      reason: expect.stringContaining("Unsupported file"),
    });
  });

  it("classifies a PDF by MIME type", async () => {
    const file = new File([new Uint8Array([0x25, 0x50, 0x44, 0x46])], "doc.pdf", {
      type: "application/pdf",
    });
    const result = await fileToAttachment(file);
    expect(result).toMatchObject({
      kind: "pdf",
      mimeType: "application/pdf",
      name: "doc.pdf",
    });
  });
});
