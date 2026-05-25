// Confirmation modal shown when the user picks a different workspace
// while a turn is in flight. Story 08.

export interface ConfirmSwitchModalProps {
  targetName: string;
  onConfirm: () => void;
  onCancel: () => void;
}

export function ConfirmSwitchModal(props: ConfirmSwitchModalProps) {
  return (
    <div
      class="confirm-switch-overlay"
      role="dialog"
      aria-modal="true"
      aria-labelledby="confirm-switch-title"
      onClick={(e) => {
        // Cancel on overlay backdrop click — not on the dialog itself.
        if (e.target === e.currentTarget) props.onCancel();
      }}
    >
      <div class="confirm-switch-dialog">
        <h2 id="confirm-switch-title">Switch workspace?</h2>
        <p>
          A turn is still running. Switching to <strong>{props.targetName}</strong>
          {" "}will cancel it.
        </p>
        <div class="confirm-switch-actions">
          <button
            type="button"
            class="confirm-switch-cancel"
            onClick={() => props.onCancel()}
          >
            Stay here
          </button>
          <button
            type="button"
            class="confirm-switch-confirm"
            onClick={() => props.onConfirm()}
          >
            Switch and cancel
          </button>
        </div>
      </div>
    </div>
  );
}
