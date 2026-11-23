import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { CoverageNotice } from "./CoverageNotice";
import { useUIStore } from "@/stores/uiStore";
import type { AuthStatus, MirrorStatus } from "@/stores/uiStore";

const PUBKY = "test-pubky";
const HOMESERVER = "8pinxxgqs41n4aididenw5apqp1urfmzdztr8jt4abrkdn435ewo";
const SIGNED_IN: AuthStatus = { type: "SignedIn" };
const NOT_MIRRORED: MirrorStatus = { type: "NotMirrored" };

function renderNotice(auth: AuthStatus, mirror: MirrorStatus = NOT_MIRRORED) {
  useUIStore.setState({
    keyStates: {
      [PUBKY]: {
        status: { type: "Idle" },
        data_size: 0,
        last_sync: null,
        next_sync: null,
        error: null,
        auth,
        mirror,
      },
    },
  });
  const onOpen = vi.fn();
  const view = render(<CoverageNotice pubky={PUBKY} onOpen={onOpen} />);
  return { ...view, onOpen };
}

describe("CoverageNotice", () => {
  it("should show nothing while private data is backed up and nothing needs attention", () => {
    const { container } = renderNotice(SIGNED_IN, {
      type: "Mirroring",
      homeserver: HOMESERVER,
    });
    expect(container).toBeEmptyDOMElement();
  });

  it("should offer to set up private backup when signed out", () => {
    renderNotice({ type: "SignedOut" });
    expect(
      screen.getByText("Private data isn't backed up."),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Set up" })).toBeInTheDocument();
  });

  it("should warn when the session expired", () => {
    renderNotice({ type: "SessionExpired" });
    expect(screen.getByText(/your session expired/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Fix" })).toBeInTheDocument();
  });

  it("should warn when the sign-in failed", () => {
    renderNotice({ type: "SignInFailed", message: "denied" });
    expect(screen.getByText(/Sign-in failed/)).toBeInTheDocument();
  });

  it("should point to a pending sign-in approval", () => {
    renderNotice({
      type: "AwaitingApproval",
      authorization_url: "pubkyauth://example",
    });
    expect(screen.getByText(/sign-in is waiting/)).toBeInTheDocument();
  });

  it("should warn when the mirror is paused", () => {
    renderNotice(SIGNED_IN, {
      type: "SessionExpired",
      homeserver: HOMESERVER,
    });
    expect(screen.getByText(/Mirroring is paused/)).toBeInTheDocument();
  });

  it("should warn when the mirror is failing", () => {
    renderNotice(SIGNED_IN, {
      type: "Failing",
      homeserver: HOMESERVER,
      message: "timeout",
    });
    expect(screen.getByText("Mirroring isn't working.")).toBeInTheDocument();
  });

  it("should warn when setting up the mirror failed", () => {
    renderNotice(SIGNED_IN, {
      type: "SetupFailed",
      homeserver: HOMESERVER,
      message: "unreachable",
    });
    expect(screen.getByText("Mirroring isn't working.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Fix" })).toBeInTheDocument();
  });

  it("should point to a pending mirror approval", () => {
    renderNotice(SIGNED_IN, {
      type: "AwaitingApproval",
      homeserver: HOMESERVER,
      authorization_url: "pubkyauth://example",
    });
    expect(screen.getByText(/mirror is waiting/)).toBeInTheDocument();
  });

  it("should point to a pending approval before an expired session", () => {
    renderNotice(
      { type: "SessionExpired" },
      {
        type: "AwaitingApproval",
        homeserver: HOMESERVER,
        authorization_url: "pubkyauth://example",
      },
    );
    expect(screen.getByText(/mirror is waiting/)).toBeInTheDocument();
    expect(screen.queryByText(/your session expired/)).not.toBeInTheDocument();
  });

  it("should open the controls when its button is clicked", () => {
    const { onOpen } = renderNotice({ type: "SignedOut" });
    fireEvent.click(screen.getByRole("button", { name: "Set up" }));
    expect(onOpen).toHaveBeenCalledOnce();
  });
});
