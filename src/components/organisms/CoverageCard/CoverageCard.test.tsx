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
import type { AuthStatus, KeyState } from "@/stores/uiStore";
import * as services from "@/services";

vi.mock("@/services", () => ({
  startSignIn: vi.fn(),
  cancelSignIn: vi.fn(),
  signOut: vi.fn(),
}));

const mockWriteText = vi.fn().mockResolvedValue(undefined);
Object.defineProperty(navigator, "clipboard", {
  value: { writeText: mockWriteText },
  writable: true,
  configurable: true,
});

const PUBKY = "test-pubky";
const AUTHORIZATION_URL = "pubkyauth://example?secret=abc";

function renderWithAuth(auth: AuthStatus) {
  const keyState: KeyState = {
    status: { type: "Idle" },
    data_size: 1024,
    last_sync: null,
    next_sync: null,
    error: null,
    total_files: null,
    files_synced: null,
    bytes_downloaded: null,
    auth,
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

  it("should always show public data as backed up", () => {
    renderWithAuth({ type: "SignedOut" });

    expect(screen.getByText("Public data")).toBeInTheDocument();
    expect(screen.getByText(/No sign-in needed/i)).toBeInTheDocument();
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
      vi.mocked(services.startSignIn).mockResolvedValue(AUTHORIZATION_URL);
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
      expect(screen.queryByText("Public data")).not.toBeInTheDocument();
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
      vi.mocked(services.startSignIn).mockResolvedValue(AUTHORIZATION_URL);
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
      vi.mocked(services.startSignIn).mockResolvedValue(AUTHORIZATION_URL);
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
});
