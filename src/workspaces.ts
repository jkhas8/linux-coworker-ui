// Typed wrappers around the workspace + conversation Tauri commands so
// components don't sprinkle `invoke<unknown>` calls everywhere. Each
// function mirrors a backend command in src-tauri/src/lib.rs.

import { invoke } from "@tauri-apps/api/core";
import type { ConversationSummary, Workspace } from "./types";

export async function listWorkspaces(): Promise<Workspace[]> {
  return invoke<Workspace[]>("list_workspaces");
}

export async function createWorkspace(
  name: string,
  path: string,
): Promise<Workspace> {
  return invoke<Workspace>("create_workspace", { name, path });
}

export async function renameWorkspace(
  id: string,
  newName: string,
): Promise<Workspace> {
  return invoke<Workspace>("rename_workspace", { id, newName });
}

export async function deleteWorkspace(id: string): Promise<string | null> {
  // Returns the new active workspace id if the deleted one was active.
  return invoke<string | null>("delete_workspace", { id });
}

export async function setActiveWorkspace(id: string): Promise<Workspace> {
  return invoke<Workspace>("set_active_workspace", { id });
}

export async function getActiveWorkspace(): Promise<Workspace | null> {
  return invoke<Workspace | null>("get_active_workspace");
}

export async function listConversations(
  workspaceId: string,
): Promise<ConversationSummary[]> {
  return invoke<ConversationSummary[]>("list_conversations", {
    workspaceId,
  });
}

export async function deleteConversation(
  workspaceId: string,
  conversationId: string,
): Promise<void> {
  await invoke("delete_conversation", { workspaceId, conversationId });
}

export async function renameConversation(
  workspaceId: string,
  conversationId: string,
  newTitle: string,
): Promise<ConversationSummary> {
  return invoke<ConversationSummary>("rename_conversation", {
    workspaceId,
    conversationId,
    newTitle,
  });
}
