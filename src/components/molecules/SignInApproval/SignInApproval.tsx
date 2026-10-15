import { useEffect, useState, type ReactNode } from "react";
import * as Atoms from "@/components/atoms";
import * as Stores from "@/stores";
import { Logger } from "@/lib";

const COPIED_LABEL_DURATION_MS = 2000;

export interface SignInApprovalProps {
  authorizationUrl: string;
  instructions: string;
  /** Shown next to the copy button, e.g. to cancel the sign-in */
  action: ReactNode;
}

/** The QR code and link the owner approves a sign-in with in Pubky Ring. */
export function SignInApproval({
  authorizationUrl,
  instructions,
  action,
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

  return (
    <div className="flex items-center gap-4 text-left">
      <Atoms.QrCode
        value={authorizationUrl}
        label="Sign-in QR code"
        size={128}
        className="shrink-0"
      />
      <div className="flex flex-col items-start gap-3">
        <span className="text-sm text-text-light">{instructions}</span>
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
          {action}
        </div>
      </div>
    </div>
  );
}
