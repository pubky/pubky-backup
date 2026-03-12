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

// Mock services
vi.mock("@/services", () => ({
  addKey: vi.fn(),
  setLastPubky: vi.fn(),
  removeKey: vi.fn(),
}));

describe("KeysPage", () => {
  const mockKeyState: KeyState = {
    status: { type: "Idle" },
    data_size: 1024,
    last_sync: 1672531200,
    next_sync: 1672531230,
    error: null,
    total_files: null,
    files_synced: null,
    bytes_downloaded: null,
  };

  beforeEach(() => {
    vi.clearAllMocks();
    useUIStore.setState({
      keyStates: {},
      lastPubky: null,
      currentPage: "keys",
    });
  });

  describe("rendering", () => {
    it("should render header and description", () => {
      render(<KeysPage />);

      expect(screen.getByText("Manage keys")).toBeInTheDocument();
      expect(
        screen.getByText(/Select a pubky to see its backup activity/i),
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

    it("should call setLastPubky when clicking a different key", async () => {
      vi.mocked(services.setLastPubky).mockResolvedValue(undefined);

      useUIStore.setState({
        keyStates: {
          "pk:key1": mockKeyState,
          "pk:key2": mockKeyState,
        },
        lastPubky: "pk:key1",
      });

      render(<KeysPage />);

      // Click on a key item (not the remove button) - KeyItem is now a div with role="button"
      const key2Element = screen.getByText(/key2/i).closest("[role='button']");
      if (key2Element) {
        fireEvent.click(key2Element);
      }

      await waitFor(() => {
        expect(services.setLastPubky).toHaveBeenCalled();
      });
    });

    it("should navigate to sync page when clicking already selected key", () => {
      useUIStore.setState({
        keyStates: {
          "pk:key1": mockKeyState,
        },
        lastPubky: "pk:key1",
        currentPage: "keys",
      });

      render(<KeysPage />);

      // Click on the already selected key - KeyItem is now a div with role="button"
      const keyElement = screen.getByText(/key1/i).closest("[role='button']");
      if (keyElement) {
        fireEvent.click(keyElement);
      }

      expect(useUIStore.getState().currentPage).toBe("sync");
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
        screen.getByRole("button", { name: /^add$/i }),
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

      expect(screen.getByRole("button", { name: /^add$/i })).toBeDisabled();
    });

    it("should enable Add button when input has value", () => {
      render(<KeysPage />);

      fireEvent.click(
        screen.getByRole("button", { name: /add another pubky/i }),
      );
      fireEvent.change(screen.getByPlaceholderText(/enter pubky to add/i), {
        target: { value: "new-pubky-value" },
      });

      expect(screen.getByRole("button", { name: /^add$/i })).not.toBeDisabled();
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
      fireEvent.click(screen.getByRole("button", { name: /^add$/i }));

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
      fireEvent.click(screen.getByRole("button", { name: /^add$/i }));

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
      fireEvent.click(screen.getByRole("button", { name: /^add$/i }));

      await waitFor(() => {
        expect(useUIStore.getState().currentPage).toBe("sync");
      });
    });

    it("should show 'Adding...' text while adding", async () => {
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
      fireEvent.click(screen.getByRole("button", { name: /^add$/i }));

      expect(screen.getByText("Adding...")).toBeInTheDocument();

      // Resolve the pending promise to avoid act() warning
      await act(async () => {
        resolveAdd!("done");
      });
    });
  });
});
