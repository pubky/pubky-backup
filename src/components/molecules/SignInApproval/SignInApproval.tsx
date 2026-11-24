import { useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import * as Atoms from "@/components/atoms";
import * as Stores from "@/stores";
import { Logger } from "@/lib";

const COPIED_LABEL_DURATION_MS = 2000;

/** The pop-ups currently open, the topmost last */
const openPopups: object[] = [];

export interface SignInApprovalProps {
  authorizationUrl: string;
  /** What the approval is for */
  title: string;
  /** Shown under the title, if given */
  instructions?: string;
  /** Cancels the request; also called on Escape */
  onCancel: () => void;
  cancelDisabled?: boolean;
}

/**
 * A pop-up with the QR code and link the owner approves a request with in
 * Pubky Ring. It stays open while it is rendered, so render it while the
 * request awaits approval.
 */
export function SignInApproval({
  authorizationUrl,
  title,
  instructions,
  onCancel,
  cancelDisabled = false,
}: SignInApprovalProps) {
  const [isCopied, setIsCopied] = useState(false);

  useEffect(() => {
    if (!isCopied) return;
    const timeout = setTimeout(
      () => setIsCopied(false),
      COPIED_LABEL_DURATION_MS,
    );
    return () => clearTimeout(timeout);
  }, [isCopied]);

  const popup = useRef({});
  const dialogRef = useRef<HTMLDivElement>(null);

  // While any pop-up is open the app behind it can't be clicked or focused,
  // so the keyboard can't leave the pop-up either
  useEffect(() => {
    const token = popup.current;
    const app = document.getElementById("root");
    openPopups.push(token);
    app?.setAttribute("inert", "");
    dialogRef.current?.focus();
    return () => {
      openPopups.splice(openPopups.indexOf(token), 1);
      if (openPopups.length === 0) app?.removeAttribute("inert");
    };
  }, []);

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      // Only the topmost pop-up, if several are open
      if (openPopups[openPopups.length - 1] !== popup.current) return;
      if (e.key === "Escape" && !cancelDisabled) onCancel();
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [onCancel, cancelDisabled]);

  const handleCopy = () => {
    navigator.clipboard
      .writeText(authorizationUrl)
      .then(() => setIsCopied(true))
      .catch((error: unknown) => {
        Logger.error("SignInApproval", "Failed to copy sign-in link", {
          error,
        });
        Stores.useUIStore
          .getState()
          .showErrorToast("Failed to copy to clipboard");
      });
  };

  // On the body, so no page's scrolling or layout can clip it
  return createPortal(
    <div className="fixed inset-0 z-40 flex items-center justify-center p-6 bg-black/70">
      <div
        ref={dialogRef}
        tabIndex={-1}
        role="dialog"
        aria-modal="true"
        aria-label={title}
        className="flex flex-col items-center gap-4 w-full max-w-sm p-6 rounded-lg bg-surface-light text-center outline-none"
      >
        <h3 className="text-base font-bold text-white m-0">{title}</h3>
        {instructions && (
          <p className="text-sm text-text-light m-0">{instructions}</p>
        )}
        <Atoms.QrCode
          value={authorizationUrl}
          label="Sign-in QR code"
          size={200}
        />
        <div className="flex gap-2">
          <Atoms.Button
            variant="secondary"
            onClick={handleCopy}
            className="py-2 px-4"
          >
            <span className="text-white text-sm font-medium">
              {isCopied ? "Link copied" : "Copy link"}
            </span>
          </Atoms.Button>
          <Atoms.Button
            variant="secondary"
            onClick={onCancel}
            disabled={cancelDisabled}
            className="py-2 px-4"
          >
            <span className="text-white text-sm font-medium">Cancel</span>
          </Atoms.Button>
        </div>
      </div>
    </div>,
    document.body,
  );
}
