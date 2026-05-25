// Empty state shown when no workspaces exist yet. Single CTA to create
// the first one; until that happens the composer is hidden.

import { createSignal, Show } from "solid-js";
import type { Workspace } from "../types";

export interface FirstLaunchProps {
  onCreate: (name: string, path: string) => Promise<Workspace>;
}

export function FirstLaunch(props: FirstLaunchProps) {
  const [open, setOpen] = createSignal(false);
  const [name, setName] = createSignal("");
  const [path, setPath] = createSignal("");
  const [error, setError] = createSignal<string | null>(null);
  const [busy, setBusy] = createSignal(false);

  async function submit(e: Event) {
    e.preventDefault();
    if (busy()) return;
    setError(null);
    setBusy(true);
    try {
      await props.onCreate(name().trim(), path().trim());
      setOpen(false);
      setName("");
      setPath("");
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div class="first-launch">
      <div class="first-launch-card">
        <h2>Create your first workspace</h2>
        <p>
          A workspace points Claude Code at a folder on your machine.
          Conversations and their history are saved under each workspace
          so you can come back to them.
        </p>
        <Show
          when={open()}
          fallback={
            <button
              type="button"
              class="first-launch-cta"
              onClick={() => setOpen(true)}
            >
              + Create workspace
            </button>
          }
        >
          <form class="first-launch-form" onSubmit={submit}>
            <label>
              <span>Name</span>
              <input
                type="text"
                value={name()}
                onInput={(e) => setName(e.currentTarget.value)}
                placeholder="my-app"
                autofocus
                required
              />
            </label>
            <label>
              <span>Folder (absolute path)</span>
              <input
                type="text"
                value={path()}
                onInput={(e) => setPath(e.currentTarget.value)}
                placeholder="/home/you/code/my-app"
                required
              />
            </label>
            <Show when={error()}>
              <div class="first-launch-error">{error()}</div>
            </Show>
            <div class="first-launch-actions">
              <button
                type="button"
                class="first-launch-cancel"
                onClick={() => setOpen(false)}
                disabled={busy()}
              >
                Cancel
              </button>
              <button
                type="submit"
                class="first-launch-submit"
                disabled={busy() || !name().trim() || !path().trim()}
              >
                {busy() ? "Creating…" : "Create"}
              </button>
            </div>
          </form>
        </Show>
      </div>
    </div>
  );
}
