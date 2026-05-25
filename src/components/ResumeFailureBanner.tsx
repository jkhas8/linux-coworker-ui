// Banner shown above the chat when Claude Code fails to resume a
// conversation. Offers a single "Continue fresh" action that starts a
// new Claude session while keeping the same conversation file on disk.

export interface ResumeFailureBannerProps {
  onContinueFresh: () => void;
  onDismiss: () => void;
}

export function ResumeFailureBanner(props: ResumeFailureBannerProps) {
  return (
    <div class="resume-failure-banner" role="alert">
      <div class="resume-failure-text">
        This conversation can't be resumed. Continue it in a fresh Claude
        session?
      </div>
      <div class="resume-failure-actions">
        <button
          type="button"
          class="resume-failure-dismiss"
          onClick={() => props.onDismiss()}
        >
          Dismiss
        </button>
        <button
          type="button"
          class="resume-failure-confirm"
          onClick={() => props.onContinueFresh()}
        >
          Continue fresh
        </button>
      </div>
    </div>
  );
}
