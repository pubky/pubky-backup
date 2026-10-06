import { describe, it, expect, beforeEach, vi } from "vitest";
import {
  render,
  screen,
  fireEvent,
  waitFor,
  act,
} from "@testing-library/react";
import { CoverageCard } from "./CoverageCard";
import { useUIStore } from "@/stores/uiStore";
import type { AuthStatus, KeyState, MirrorStatus } from "@/stores/uiStore";
import * as services from "@/services";
import { displayPubky } from "@/utils";

vi.mock("@/services", () => ({
  startSignIn: vi.fn(),
  cancelSignIn: vi.fn(),
  signOut: vi.fn(),
  startMirror: vi.fn(),
  cancelMirror: vi.fn(),
  stopMirror: vi.fn(),
}));

const mockWriteText = vi.fn().mockResolvedValue(undefined);
Object.defineProperty(navigator, "clipboard", {
  value: { writeText: mockWriteText },
  writable: true,
  configurable: true,
});

const PUBKY = "test-pubky";
const AUTHORIZATION_URL = "pubkyauth://example?secret=abc";
const HOMESERVER = "8pinxxgqs41n4aididenw5apqp1urfmzdztr8jt4abrkdn435ewo";
const HOMESERVER_SHORT = displayPubky(HOMESERVER);

function renderWithAuth(
  auth: AuthStatus,
  mirror: MirrorStatus = { type: "NotMirrored" },
) {
  const keyState: KeyState = {
    status: { type: "Idle" },
    data_size: 1024,
    last_sync: null,
    next_sync: null,
    error: null,
    auth,
    mirror,
  };
  useUIStore.setState({ keyStates: { [PUBKY]: keyState } });
  return render(<CoverageCard pubky={PUBKY} />);
}

describe("CoverageCard", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useUIStore.setState({
      keyStates: {},
      toast: { visible: false, pubkyText: "", type: "success", message: "" },
    });
  });

  it("should treat a key without state yet as signed out", () => {
    render(<CoverageCard pubky={PUBKY} />);

    expect(screen.getByRole("button", { name: "Sign in" })).toBeInTheDocument();
  });

  describe("signed out", () => {
    it("should explain that private data is not backed up", () => {
      renderWithAuth({ type: "SignedOut" });

      expect(
        screen.getByText(/Not backed up\. Sign in with Pubky Ring/i),
      ).toBeInTheDocument();
      expect(screen.queryByTitle("Sign-in QR code")).not.toBeInTheDocument();
    });

    it("should start a sign-in when Sign in is clicked", async () => {
      vi.mocked(services.startSignIn).mockResolvedValue(undefined);
      renderWithAuth({ type: "SignedOut" });

      fireEvent.click(screen.getByRole("button", { name: "Sign in" }));

      await waitFor(() => {
        expect(services.startSignIn).toHaveBeenCalledWith(PUBKY);
      });
    });

    it("should show an error toast when the sign-in cannot be started", async () => {
      vi.mocked(services.startSignIn).mockRejectedValue({
        type: "Internal",
        message: "Authentication failed: relay unreachable",
      });
      renderWithAuth({ type: "SignedOut" });

      fireEvent.click(screen.getByRole("button", { name: "Sign in" }));

      await waitFor(() => {
        const { toast } = useUIStore.getState();
        expect(toast.visible).toBe(true);
        expect(toast.type).toBe("error");
        expect(toast.message).toContain("relay unreachable");
      });
    });
  });

  describe("awaiting approval", () => {
    const awaiting: AuthStatus = {
      type: "AwaitingApproval",
      authorization_url: AUTHORIZATION_URL,
    };

    it("should show the QR code to scan", () => {
      renderWithAuth(awaiting);

      expect(screen.getByTitle("Sign-in QR code")).toBeInTheDocument();
      expect(screen.getByText(/only grants read access/i)).toBeInTheDocument();
    });

    it("should show only the approval step, to fit the window", () => {
      renderWithAuth(awaiting);

      expect(
        screen.getByText("Sign in to back up private data"),
      ).toBeInTheDocument();
      expect(screen.queryByText("Private data")).not.toBeInTheDocument();
    });

    it("should copy the sign-in link", async () => {
      renderWithAuth(awaiting);

      fireEvent.click(screen.getByRole("button", { name: "Copy link" }));

      expect(mockWriteText).toHaveBeenCalledWith(AUTHORIZATION_URL);
      expect(
        await screen.findByRole("button", { name: "Link copied" }),
      ).toBeInTheDocument();
    });

    it("should cancel the sign-in when Cancel is clicked", async () => {
      vi.mocked(services.cancelSignIn).mockResolvedValue(undefined);
      renderWithAuth(awaiting);

      fireEvent.click(screen.getByRole("button", { name: "Cancel" }));

      await waitFor(() => {
        expect(services.cancelSignIn).toHaveBeenCalledWith(PUBKY);
      });
    });
  });

  describe("signed in", () => {
    it("should show private data as backed up", () => {
      renderWithAuth({ type: "SignedIn" });

      expect(
        screen.getByText(/Backed up while you stay signed in/i),
      ).toBeInTheDocument();
      expect(screen.queryByTitle("Sign-in QR code")).not.toBeInTheDocument();
    });

    it("should sign out when Sign out is clicked", async () => {
      vi.mocked(services.signOut).mockResolvedValue(undefined);
      renderWithAuth({ type: "SignedIn" });

      fireEvent.click(screen.getByRole("button", { name: "Sign out" }));

      await waitFor(() => {
        expect(services.signOut).toHaveBeenCalledWith(PUBKY);
      });
    });
  });

  describe("session expired", () => {
    it("should ask to sign in again", async () => {
      vi.mocked(services.startSignIn).mockResolvedValue(undefined);
      renderWithAuth({ type: "SessionExpired" });

      expect(screen.getByText(/your session expired/i)).toBeInTheDocument();
      fireEvent.click(screen.getByRole("button", { name: "Sign in again" }));

      await waitFor(() => {
        expect(services.startSignIn).toHaveBeenCalledWith(PUBKY);
      });
    });
  });

  describe("sign-in failed", () => {
    it("should show the reason and offer to try again", async () => {
      vi.mocked(services.startSignIn).mockResolvedValue(undefined);
      renderWithAuth({
        type: "SignInFailed",
        message: "The sign-in was approved by a different key",
      });

      expect(
        screen.getByText(/approved by a different key/i),
      ).toBeInTheDocument();
      fireEvent.click(screen.getByRole("button", { name: "Try again" }));

      await waitFor(() => {
        expect(services.startSignIn).toHaveBeenCalledWith(PUBKY);
      });
    });
  });

  it("should follow the key's state as the backend updates it", () => {
    renderWithAuth({ type: "SignedOut" });
    expect(screen.getByRole("button", { name: "Sign in" })).toBeInTheDocument();

    // The backend reports the approval through a key-update
    const current = useUIStore.getState().keyStates[PUBKY]!;
    act(() => {
      useUIStore
        .getState()
        .setKeyState(PUBKY, { ...current, auth: { type: "SignedIn" } });
    });

    expect(
      screen.getByRole("button", { name: "Sign out" }),
    ).toBeInTheDocument();
  });

  describe("mirror", () => {
    const signedOut: AuthStatus = { type: "SignedOut" };

    it("should offer to add a mirror when there is none", () => {
      renderWithAuth(signedOut);

      expect(screen.getByText("Mirror")).toBeInTheDocument();
      expect(screen.getByText(/Not mirrored/i)).toBeInTheDocument();
      expect(
        screen.getByRole("button", { name: "Add mirror" }),
      ).toBeInTheDocument();
    });

    it("should ask for the homeserver and request approval", async () => {
      vi.mocked(services.startMirror).mockResolvedValue(AUTHORIZATION_URL);
      renderWithAuth(signedOut);

      fireEvent.click(screen.getByRole("button", { name: "Add mirror" }));

      expect(
        screen.getByText("Mirror to another homeserver"),
      ).toBeInTheDocument();
      const request = screen.getByRole("button", { name: "Request approval" });
      expect(request).toBeDisabled();

      fireEvent.change(screen.getByPlaceholderText("Homeserver key"), {
        target: { value: ` ${HOMESERVER} ` },
      });
      fireEvent.click(request);

      await waitFor(() => {
        expect(services.startMirror).toHaveBeenCalledWith(PUBKY, HOMESERVER);
      });
      // The form closes; the QR code follows in the key's state
      expect(
        screen.queryByPlaceholderText("Homeserver key"),
      ).not.toBeInTheDocument();
    });

    it("should keep the form open when the request fails", async () => {
      vi.mocked(services.startMirror).mockRejectedValue({
        type: "Internal",
        message: "Mirroring failed: own homeserver",
      });
      renderWithAuth(signedOut);
      fireEvent.click(screen.getByRole("button", { name: "Add mirror" }));
      fireEvent.change(screen.getByPlaceholderText("Homeserver key"), {
        target: { value: HOMESERVER },
      });

      fireEvent.click(screen.getByRole("button", { name: "Request approval" }));

      await waitFor(() => {
        expect(useUIStore.getState().toast.message).toContain("own homeserver");
      });
      expect(screen.getByPlaceholderText("Homeserver key")).toBeInTheDocument();
    });

    it("should close the form on cancel", () => {
      renderWithAuth(signedOut);
      fireEvent.click(screen.getByRole("button", { name: "Add mirror" }));

      fireEvent.click(screen.getByRole("button", { name: "Cancel" }));

      expect(
        screen.getByRole("button", { name: "Add mirror" }),
      ).toBeInTheDocument();
    });

    it("should show the QR code while the mirror awaits approval", async () => {
      vi.mocked(services.cancelMirror).mockResolvedValue(undefined);
      renderWithAuth(signedOut, {
        type: "AwaitingApproval",
        homeserver: HOMESERVER,
        authorization_url: AUTHORIZATION_URL,
      });

      expect(screen.getByText("Approve the mirror")).toBeInTheDocument();
      expect(screen.getByTitle("Sign-in QR code")).toBeInTheDocument();
      expect(screen.getByText(/grants write access/i)).toBeInTheDocument();
      expect(screen.queryByText("Public data")).not.toBeInTheDocument();

      fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
      await waitFor(() => {
        expect(services.cancelMirror).toHaveBeenCalledWith(PUBKY);
      });
    });

    it("should show the sign-in request first when both await approval", () => {
      renderWithAuth(
        { type: "AwaitingApproval", authorization_url: AUTHORIZATION_URL },
        {
          type: "AwaitingApproval",
          homeserver: HOMESERVER,
          authorization_url: "pubkyauth://mirror",
        },
      );

      expect(
        screen.getByText("Sign in to back up private data"),
      ).toBeInTheDocument();
    });

    it("should show a working mirror and let the user stop it", async () => {
      vi.mocked(services.stopMirror).mockResolvedValue(undefined);
      renderWithAuth(signedOut, { type: "Mirroring", homeserver: HOMESERVER });

      expect(
        screen.getByText(`Copied to ${HOMESERVER_SHORT} after every sync.`),
      ).toBeInTheDocument();
      fireEvent.click(screen.getByRole("button", { name: "Stop" }));

      await waitFor(() => {
        expect(services.stopMirror).toHaveBeenCalledWith(PUBKY);
      });
    });

    it("should ask to approve again when the mirror session expired", async () => {
      vi.mocked(services.startMirror).mockResolvedValue(AUTHORIZATION_URL);
      renderWithAuth(signedOut, {
        type: "SessionExpired",
        homeserver: HOMESERVER,
      });

      expect(screen.getByText(/Paused:/i)).toBeInTheDocument();
      fireEvent.click(screen.getByRole("button", { name: "Approve again" }));

      await waitFor(() => {
        expect(services.startMirror).toHaveBeenCalledWith(PUBKY, HOMESERVER);
      });
    });

    it("should show why copying is failing", () => {
      renderWithAuth(signedOut, {
        type: "Failing",
        homeserver: HOMESERVER,
        message: "The mirror responded with 507 Insufficient Storage",
      });

      expect(
        screen.getByText(/is failing: The mirror responded with 507/i),
      ).toBeInTheDocument();
      expect(screen.getByRole("button", { name: "Stop" })).toBeInTheDocument();
    });

    it("should reopen the form with the homeserver after a failed setup", () => {
      renderWithAuth(signedOut, {
        type: "SetupFailed",
        homeserver: HOMESERVER,
        message: "The key has no account on this homeserver",
      });

      expect(screen.getByText(/no account/i)).toBeInTheDocument();
      fireEvent.click(screen.getByRole("button", { name: "Try again" }));

      expect(screen.getByPlaceholderText("Homeserver key")).toHaveValue(
        HOMESERVER,
      );
    });
  });
});
