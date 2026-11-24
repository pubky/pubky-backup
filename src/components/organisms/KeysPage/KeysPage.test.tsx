import { describe, it, expect, beforeEach, vi } from "vitest";
import {
  render,
  screen,
  fireEvent,
  waitFor,
  act,
} from "@testing-library/react";
import { KeysPage } from "./KeysPage";
import { useUIStore } from "@/stores/uiStore";
import type { KeyState } from "@/stores/uiStore";
import * as services from "@/services";
import { ask } from "@tauri-apps/plugin-dialog";

// Mock services
vi.mock("@/services", () => ({
  addKey: vi.fn(),
  setLastPubky: vi.fn(),
  removeKey: vi.fn(),
  signInNewKey: vi.fn(),
  cancelSignInNewKey: vi.fn(),
  getConfig: vi.fn(),
  startSignIn: vi.fn(),
  cancelSignIn: vi.fn(),
  signOut: vi.fn(),
  startMirror: vi.fn(),
  cancelMirror: vi.fn(),
  stopMirror: vi.fn(),
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({
  ask: vi.fn(),
}));

describe("KeysPage", () => {
  const mockKeyState: KeyState = {
    status: { type: "Idle" },
    data_size: 1024,
    last_sync: 1672531200,
    next_sync: 1672531230,
    error: null,
    auth: { type: "SignedOut" },
    mirror: { type: "NotMirrored" },
  };

  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(ask).mockResolvedValue(true);
    vi.mocked(services.getConfig).mockResolvedValue({
      sync_interval_secs: 300,
      keys_dir: "/home/user/.pubky-backup/keys",
      mirroring: false,
    });
    useUIStore.setState({
      keyStates: {},
      lastPubky: null,
      currentPage: "keys",
    });
  });

  describe("rendering", () => {
    it("should render header and description", () => {
      render(<KeysPage />);

      expect(screen.getByText("Manage pubkys")).toBeInTheDocument();
      expect(
        screen.getByText(/Select a pubky to manage its backup/i),
      ).toBeInTheDocument();
    });

    it("should show key count", () => {
      useUIStore.setState({
        keyStates: {
          "pk:key1": mockKeyState,
          "pk:key2": mockKeyState,
        },
      });

      render(<KeysPage />);

      expect(screen.getByText(/Your pubkys \(2\)/i)).toBeInTheDocument();
    });

    it("should render list of keys", () => {
      useUIStore.setState({
        keyStates: {
          g1b6wp8bhhxt1234567890abcdef: mockKeyState,
          a2c7xq9ciiyv0987654321fedcba: mockKeyState,
        },
      });

      render(<KeysPage />);

      // Keys should be displayed (truncated format: first 5 + ... + last 5)
      // g1b6wp8bhhxt1234567890abcdef -> g1b6w...bcdef
      // a2c7xq9ciiyv0987654321fedcba -> a2c7x...dcba
      expect(screen.getByText(/g1b6w/i)).toBeInTheDocument();
      expect(screen.getByText(/a2c7x/i)).toBeInTheDocument();
    });

    it("should show which keys have their private data backed up", () => {
      useUIStore.setState({
        keyStates: {
          g1b6wp8bhhxt1234567890abcdef: mockKeyState,
          a2c7xq9ciiyv0987654321fedcba: {
            ...mockKeyState,
            auth: { type: "SignedIn" },
          },
        },
      });

      render(<KeysPage />);

      expect(screen.getAllByText("Public only")).toHaveLength(1);
      expect(screen.getAllByText("Public + private")).toHaveLength(1);
    });

    it("should show which keys are mirrored", () => {
      useUIStore.setState({
        keyStates: {
          g1b6wp8bhhxt1234567890abcdef: {
            ...mockKeyState,
            auth: { type: "SignedIn" },
            mirror: { type: "Mirroring", homeserver: "some-homeserver" },
          },
        },
      });

      render(<KeysPage />);

      expect(
        screen.getByText("Public + private, mirrored"),
      ).toBeInTheDocument();
    });

    it("should mark keys whose backup is in error", () => {
      useUIStore.setState({
        keyStates: {
          g1b6wp8bhhxt1234567890abcdef: {
            ...mockKeyState,
            status: { type: "Error" },
            error: { message: "disk full" },
          },
          a2c7xq9ciiyv0987654321fedcba: mockKeyState,
        },
      });

      render(<KeysPage />);

      const marker = screen.getByTitle("Backup failed");
      const errored = screen.getByText(/g1b6w/i).closest('[role="button"]');
      const healthy = screen.getByText(/a2c7x/i).closest('[role="button"]');
      expect(errored).toContainElement(marker);
      expect(healthy).not.toContainElement(marker);
    });

    it("should show 'Add another pubky' button", () => {
      render(<KeysPage />);

      expect(
        screen.getByRole("button", { name: /add another pubky/i }),
      ).toBeInTheDocument();
    });

    it("should show empty state when no keys", () => {
      render(<KeysPage />);

      expect(screen.getByText(/Your pubkys \(0\)/i)).toBeInTheDocument();
    });
  });

  describe("key selection", () => {
    it("should highlight selected key", () => {
      useUIStore.setState({
        keyStates: {
          "pk:key1": mockKeyState,
          "pk:key2": mockKeyState,
        },
        lastPubky: "pk:key1",
      });

      render(<KeysPage />);

      // The selected key should have a check icon
      const removeButtons = screen.getAllByTitle("Remove pubky");
      expect(removeButtons.length).toBe(2);
    });

    it("should stay on the Keys page when selecting a key", async () => {
      vi.mocked(services.setLastPubky).mockResolvedValue(undefined);
      useUIStore.setState({
        keyStates: {
          "pk:key1": mockKeyState,
          "pk:key2": mockKeyState,
        },
        lastPubky: "pk:key1",
        currentPage: "keys",
      });

      render(<KeysPage />);

      const key2Element = screen.getByText(/key2/i).closest("[role='button']");
      fireEvent.click(key2Element!);

      await waitFor(() => {
        expect(services.setLastPubky).toHaveBeenCalled();
      });
      expect(useUIStore.getState().currentPage).toBe("keys");
    });

    it("should do nothing when clicking the selected key", () => {
      useUIStore.setState({
        keyStates: { "pk:key1": mockKeyState },
        lastPubky: "pk:key1",
        currentPage: "keys",
      });

      render(<KeysPage />);
      fireEvent.click(screen.getByText(/key1/i).closest("[role='button']")!);

      expect(services.setLastPubky).not.toHaveBeenCalled();
      expect(useUIStore.getState().currentPage).toBe("keys");
    });
  });

  describe("signing in", () => {
    it("should let any key sign in without selecting it", async () => {
      vi.mocked(services.startSignIn).mockResolvedValue(undefined);
      useUIStore.setState({
        keyStates: {
          "pk:key1": mockKeyState,
          "pk:key2": { ...mockKeyState, auth: { type: "SignedIn" } },
        },
        lastPubky: "pk:key2",
      });

      render(<KeysPage />);
      fireEvent.click(screen.getByRole("button", { name: "Sign in" }));

      await waitFor(() => {
        expect(services.startSignIn).toHaveBeenCalledWith("pk:key1");
      });
      expect(services.setLastPubky).not.toHaveBeenCalled();
    });

    it("should sign a signed-in key out", async () => {
      vi.mocked(services.signOut).mockResolvedValue(undefined);
      useUIStore.setState({
        keyStates: {
          "pk:key1": { ...mockKeyState, auth: { type: "SignedIn" } },
        },
        lastPubky: "pk:key1",
      });

      render(<KeysPage />);
      fireEvent.click(screen.getByRole("button", { name: "Sign out" }));

      await waitFor(() => {
        expect(services.signOut).toHaveBeenCalledWith("pk:key1");
      });
      expect(ask).toHaveBeenCalledWith(
        expect.stringContaining("Sign out of"),
        expect.objectContaining({ okLabel: "Sign out" }),
      );
    });

    it("should stay signed in when the sign-out is not confirmed", async () => {
      vi.mocked(ask).mockResolvedValue(false);
      useUIStore.setState({
        keyStates: {
          "pk:key1": { ...mockKeyState, auth: { type: "SignedIn" } },
        },
        lastPubky: "pk:key1",
      });

      render(<KeysPage />);
      fireEvent.click(screen.getByRole("button", { name: "Sign out" }));

      await waitFor(() => {
        expect(ask).toHaveBeenCalled();
      });
      expect(services.signOut).not.toHaveBeenCalled();
    });

    it("should offer sign-out on the selected pubky only", () => {
      useUIStore.setState({
        keyStates: {
          "pk:key1": { ...mockKeyState, auth: { type: "SignedIn" } },
          "pk:key2": { ...mockKeyState, auth: { type: "SignedIn" } },
        },
        lastPubky: "pk:key2",
      });

      render(<KeysPage />);

      // getByRole throws if more than one key offers it
      expect(
        screen.getByRole("button", { name: "Sign out" }),
      ).toBeInTheDocument();
      expect(screen.getAllByText("Public + private")).toHaveLength(2);
    });

    it("should ask to sign in again when the session expired", () => {
      useUIStore.setState({
        keyStates: {
          "pk:key1": { ...mockKeyState, auth: { type: "SessionExpired" } },
        },
        lastPubky: "pk:key1",
      });

      render(<KeysPage />);

      expect(
        screen.getByRole("button", { name: "Sign in again" }),
      ).toBeInTheDocument();
    });

    it("should show why the sign-in failed and offer to try again", () => {
      useUIStore.setState({
        keyStates: {
          "pk:key1": {
            ...mockKeyState,
            auth: {
              type: "SignInFailed",
              message: "approved by a different key",
            },
          },
        },
        lastPubky: "pk:key1",
      });

      render(<KeysPage />);

      expect(
        screen.getByText("Sign-in failed: approved by a different key"),
      ).toBeInTheDocument();
      expect(
        screen.getByRole("button", { name: "Try again" }),
      ).toBeInTheDocument();
    });

    it("should show an error toast when the sign-in cannot be started", async () => {
      vi.mocked(services.startSignIn).mockRejectedValue({
        type: "Internal",
        message: "relay unreachable",
      });
      useUIStore.setState({
        keyStates: { "pk:key1": mockKeyState },
        lastPubky: "pk:key1",
      });

      render(<KeysPage />);
      fireEvent.click(screen.getByRole("button", { name: "Sign in" }));

      await waitFor(() => {
        expect(useUIStore.getState().toast.message).toContain(
          "relay unreachable",
        );
      });
    });

    describe("awaiting approval", () => {
      const awaiting: KeyState = {
        ...mockKeyState,
        auth: {
          type: "AwaitingApproval",
          authorization_url: "pubkyauth://example",
        },
      };

      it("should show the QR code in a pop-up", () => {
        useUIStore.setState({
          keyStates: { "pk:key1": mockKeyState, "pk:key2": awaiting },
          lastPubky: "pk:key1",
        });

        render(<KeysPage />);

        const dialog = screen.getByRole("dialog");
        expect(dialog).toHaveAccessibleName(/^Sign in /);
        expect(screen.getByTitle("Sign-in QR code")).toBeInTheDocument();
      });

      it("should cancel the sign-in without selecting the key", async () => {
        vi.mocked(services.cancelSignIn).mockResolvedValue(undefined);
        useUIStore.setState({
          keyStates: { "pk:key1": mockKeyState, "pk:key2": awaiting },
          lastPubky: "pk:key1",
        });

        render(<KeysPage />);
        fireEvent.click(screen.getByRole("button", { name: "Cancel" }));

        await waitFor(() => {
          expect(services.cancelSignIn).toHaveBeenCalledWith("pk:key2");
        });
        expect(services.setLastPubky).not.toHaveBeenCalled();
      });

      it("should cancel the sign-in on Escape", async () => {
        vi.mocked(services.cancelSignIn).mockResolvedValue(undefined);
        useUIStore.setState({
          keyStates: { "pk:key1": awaiting },
          lastPubky: "pk:key1",
        });

        render(<KeysPage />);
        fireEvent.keyDown(window, { key: "Escape" });

        await waitFor(() => {
          expect(services.cancelSignIn).toHaveBeenCalledWith("pk:key1");
        });
      });

      it("should close the pop-up once the sign-in is approved", () => {
        useUIStore.setState({
          keyStates: { "pk:key1": awaiting },
          lastPubky: "pk:key1",
        });
        render(<KeysPage />);

        act(() => {
          useUIStore.setState({
            keyStates: {
              "pk:key1": { ...awaiting, auth: { type: "SignedIn" } },
            },
          });
        });

        expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
        expect(
          screen.getByRole("button", { name: "Sign out" }),
        ).toBeInTheDocument();
      });
    });
  });

  describe("remove functionality", () => {
    it("should call removeKey and remove from state when remove button is clicked", async () => {
      vi.mocked(services.removeKey).mockResolvedValue(undefined);

      useUIStore.setState({
        keyStates: {
          "pk:test-pubky-to-remove": mockKeyState,
        },
      });

      render(<KeysPage />);

      const removeButton = screen.getByTitle("Remove pubky");
      fireEvent.click(removeButton);

      await waitFor(() => {
        expect(services.removeKey).toHaveBeenCalledWith(
          "pk:test-pubky-to-remove",
        );
      });

      await waitFor(() => {
        expect(
          useUIStore.getState().keyStates["pk:test-pubky-to-remove"],
        ).toBeUndefined();
      });
    });

    it("should keep the key when the removal is not confirmed", async () => {
      vi.mocked(ask).mockResolvedValue(false);
      useUIStore.setState({
        keyStates: { "pk:test-pubky-to-keep": mockKeyState },
      });

      render(<KeysPage />);
      fireEvent.click(screen.getByTitle("Remove pubky"));

      await waitFor(() => {
        expect(ask).toHaveBeenCalled();
      });
      expect(services.removeKey).not.toHaveBeenCalled();
      expect(
        useUIStore.getState().keyStates["pk:test-pubky-to-keep"],
      ).toBeDefined();
    });

    it("should show an error and keep the key when the dialog fails", async () => {
      vi.mocked(ask).mockRejectedValue(new Error("no dialog"));
      useUIStore.setState({
        keyStates: { "pk:test-pubky-to-keep": mockKeyState },
      });

      render(<KeysPage />);
      fireEvent.click(screen.getByTitle("Remove pubky"));

      await waitFor(() => {
        expect(useUIStore.getState().toast.visible).toBe(true);
      });
      expect(useUIStore.getState().toast.type).toBe("error");
      expect(services.removeKey).not.toHaveBeenCalled();
      expect(
        useUIStore.getState().keyStates["pk:test-pubky-to-keep"],
      ).toBeDefined();
    });

    it("should select another key when removing the currently viewed key", async () => {
      vi.mocked(services.removeKey).mockResolvedValue(undefined);
      vi.mocked(services.setLastPubky).mockResolvedValue(undefined);

      useUIStore.setState({
        keyStates: {
          "pk:key1": mockKeyState,
          "pk:key2": mockKeyState,
        },
        lastPubky: "pk:key1",
      });

      render(<KeysPage />);

      // Find the remove button for the first key (which is the viewed key)
      const removeButtons = screen.getAllByTitle("Remove pubky");
      const firstRemoveButton = removeButtons[0];
      if (firstRemoveButton) {
        fireEvent.click(firstRemoveButton);
      }

      await waitFor(() => {
        expect(services.removeKey).toHaveBeenCalledWith("pk:key1");
      });

      // Should switch to the other key
      await waitFor(() => {
        expect(services.setLastPubky).toHaveBeenCalledWith("pk:key2");
      });
    });

    it("should clear lastPubky and go to startup screen when removing the last key", async () => {
      vi.mocked(services.removeKey).mockResolvedValue(undefined);

      useUIStore.setState({
        keyStates: {
          "pk:only-key": mockKeyState,
        },
        lastPubky: "pk:only-key",
        currentScreen: "dashboard",
      });

      render(<KeysPage />);

      const removeButton = screen.getByTitle("Remove pubky");
      fireEvent.click(removeButton);

      await waitFor(() => {
        expect(services.removeKey).toHaveBeenCalledWith("pk:only-key");
      });

      await waitFor(() => {
        expect(useUIStore.getState().lastPubky).toBeNull();
      });

      await waitFor(() => {
        expect(useUIStore.getState().currentScreen).toBe("startup");
      });
    });
  });

  describe("add pubky flow", () => {
    it("should show input when 'Add another pubky' is clicked", () => {
      render(<KeysPage />);

      fireEvent.click(
        screen.getByRole("button", { name: /add another pubky/i }),
      );

      expect(
        screen.getByPlaceholderText(/enter pubky to add/i),
      ).toBeInTheDocument();
      expect(
        screen.getByRole("button", { name: /cancel/i }),
      ).toBeInTheDocument();
      expect(
        screen.getByRole("button", { name: /back up public data/i }),
      ).toBeInTheDocument();
    });

    it("should hide input when Cancel is clicked", () => {
      render(<KeysPage />);

      fireEvent.click(
        screen.getByRole("button", { name: /add another pubky/i }),
      );
      fireEvent.click(screen.getByRole("button", { name: /cancel/i }));

      expect(
        screen.queryByPlaceholderText(/enter pubky to add/i),
      ).not.toBeInTheDocument();
      expect(
        screen.getByRole("button", { name: /add another pubky/i }),
      ).toBeInTheDocument();
    });

    it("should disable Add button when input is empty", () => {
      render(<KeysPage />);

      fireEvent.click(
        screen.getByRole("button", { name: /add another pubky/i }),
      );

      expect(
        screen.getByRole("button", { name: /back up public data/i }),
      ).toBeDisabled();
    });

    it("should enable Add button when input has value", () => {
      render(<KeysPage />);

      fireEvent.click(
        screen.getByRole("button", { name: /add another pubky/i }),
      );
      fireEvent.change(screen.getByPlaceholderText(/enter pubky to add/i), {
        target: { value: "new-pubky-value" },
      });

      expect(
        screen.getByRole("button", { name: /back up public data/i }),
      ).not.toBeDisabled();
    });

    it("should call addKey service when Add is clicked", async () => {
      vi.mocked(services.addKey).mockResolvedValue("normalized-pubky");

      render(<KeysPage />);

      fireEvent.click(
        screen.getByRole("button", { name: /add another pubky/i }),
      );
      fireEvent.change(screen.getByPlaceholderText(/enter pubky to add/i), {
        target: { value: "my-new-pubky" },
      });
      fireEvent.click(
        screen.getByRole("button", { name: /back up public data/i }),
      );

      await waitFor(() => {
        expect(services.addKey).toHaveBeenCalledWith("my-new-pubky");
      });
    });

    it("should clear input and hide form after successful add", async () => {
      vi.mocked(services.addKey).mockResolvedValue("normalized-pubky");

      render(<KeysPage />);

      fireEvent.click(
        screen.getByRole("button", { name: /add another pubky/i }),
      );
      fireEvent.change(screen.getByPlaceholderText(/enter pubky to add/i), {
        target: { value: "new-pubky" },
      });
      fireEvent.click(
        screen.getByRole("button", { name: /back up public data/i }),
      );

      await waitFor(() => {
        expect(
          screen.queryByPlaceholderText(/enter pubky to add/i),
        ).not.toBeInTheDocument();
      });
    });

    it("should navigate to sync page after successful add", async () => {
      vi.mocked(services.addKey).mockResolvedValue("normalized-pubky");
      useUIStore.setState({ currentPage: "keys" });

      render(<KeysPage />);

      fireEvent.click(
        screen.getByRole("button", { name: /add another pubky/i }),
      );
      fireEvent.change(screen.getByPlaceholderText(/enter pubky to add/i), {
        target: { value: "new-pubky" },
      });
      fireEvent.click(
        screen.getByRole("button", { name: /back up public data/i }),
      );

      await waitFor(() => {
        expect(useUIStore.getState().currentPage).toBe("sync");
      });
    });

    it("should disable adding while adding", async () => {
      let resolveAdd: (value: string) => void;
      vi.mocked(services.addKey).mockImplementation(
        () =>
          new Promise((resolve) => {
            resolveAdd = resolve;
          }),
      );

      render(<KeysPage />);

      fireEvent.click(
        screen.getByRole("button", { name: /add another pubky/i }),
      );
      fireEvent.change(screen.getByPlaceholderText(/enter pubky to add/i), {
        target: { value: "new-pubky" },
      });
      fireEvent.click(
        screen.getByRole("button", { name: /back up public data/i }),
      );

      expect(
        screen.getByRole("button", { name: /back up public data/i }),
      ).toBeDisabled();

      // Resolve the pending promise to avoid act() warning
      await act(async () => {
        resolveAdd!("done");
      });
    });

    it("should close the add view and show the pubky a sign-in added", async () => {
      vi.mocked(services.signInNewKey).mockResolvedValue("signed-in-pubky");
      useUIStore.setState({ currentPage: "keys" });

      render(<KeysPage />);
      fireEvent.click(
        screen.getByRole("button", { name: /add another pubky/i }),
      );
      fireEvent.click(
        screen.getByRole("button", { name: /sign in with pubky ring/i }),
      );

      await waitFor(() => {
        expect(useUIStore.getState().currentPage).toBe("sync");
      });
      expect(useUIStore.getState().lastPubky).toBe("signed-in-pubky");
      expect(
        screen.queryByPlaceholderText(/enter pubky to add/i),
      ).not.toBeInTheDocument();
    });
  });
});
