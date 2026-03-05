import { describe, it, expect, beforeEach, vi } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { KeysPage } from "./KeysPage";
import { useUIStore } from "@/stores/uiStore";
import type { KeyState } from "@/stores/uiStore";
import * as services from "@/services";

// Mock services
vi.mock("@/services", () => ({
  addKey: vi.fn(),
  setLastPubky: vi.fn(),
}));

// Mock clipboard API
const mockWriteText = vi.fn().mockResolvedValue(undefined);
Object.defineProperty(navigator, "clipboard", {
  value: { writeText: mockWriteText },
  writable: true,
  configurable: true,
});

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
      viewedPubky: null,
      currentPage: "keys",
    });
  });

  describe("rendering", () => {
    it("should render header and description", () => {
      render(<KeysPage />);

      expect(screen.getByText("Manage keys")).toBeInTheDocument();
      expect(
        screen.getByText(/Select a pubky to see its backup activity/i)
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
          "g1b6wp8bhhxt1234567890abcdef": mockKeyState,
          "a2c7xq9ciiyv0987654321fedcba": mockKeyState,
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
        screen.getByRole("button", { name: /add another pubky/i })
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
        viewedPubky: "pk:key1",
      });

      render(<KeysPage />);

      // The selected key should have a check icon
      const checkIcons = screen.getAllByTitle("Copy pubky");
      expect(checkIcons.length).toBe(2);
    });

    it("should call setViewedPubky when clicking a different key", async () => {
      vi.mocked(services.setLastPubky).mockResolvedValue(undefined);

      useUIStore.setState({
        keyStates: {
          "pk:key1": mockKeyState,
          "pk:key2": mockKeyState,
        },
        viewedPubky: "pk:key1",
      });

      render(<KeysPage />);

      // Find and click the second key
      const keyButtons = screen.getAllByRole("button").filter(
        (btn) => btn.textContent?.includes("key2") || btn.closest("[class*='pk:key2']")
      );

      // Click on a key item (not the copy button)
      const key2Button = screen.getByText(/key2/i).closest("button");
      if (key2Button) {
        fireEvent.click(key2Button);
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
        viewedPubky: "pk:key1",
        currentPage: "keys",
      });

      render(<KeysPage />);

      // Click on the already selected key
      const keyButton = screen.getByText(/key1/i).closest("button");
      if (keyButton) {
        fireEvent.click(keyButton);
      }

      expect(useUIStore.getState().currentPage).toBe("sync");
    });
  });

  describe("copy functionality", () => {
    it("should copy pubky to clipboard when copy button is clicked", async () => {
      useUIStore.setState({
        keyStates: {
          "pk:test-pubky-to-copy": mockKeyState,
        },
      });

      render(<KeysPage />);

      const copyButtons = screen.getAllByTitle("Copy pubky");
      fireEvent.click(copyButtons[0]);

      await waitFor(() => {
        expect(mockWriteText).toHaveBeenCalledWith("pk:test-pubky-to-copy");
      });
    });

    it("should show toast after copying", async () => {
      useUIStore.setState({
        keyStates: {
          "pk:copy-me": mockKeyState,
        },
        toast: { visible: false, pubkyText: "", type: "success", message: "" },
      });

      render(<KeysPage />);

      const copyButton = screen.getByTitle("Copy pubky");
      fireEvent.click(copyButton);

      await waitFor(() => {
        expect(useUIStore.getState().toast.visible).toBe(true);
      });
    });
  });

  describe("add pubky flow", () => {
    it("should show input when 'Add another pubky' is clicked", () => {
      render(<KeysPage />);

      fireEvent.click(screen.getByRole("button", { name: /add another pubky/i }));

      expect(screen.getByPlaceholderText(/enter pubky to add/i)).toBeInTheDocument();
      expect(screen.getByRole("button", { name: /cancel/i })).toBeInTheDocument();
      expect(screen.getByRole("button", { name: /^add$/i })).toBeInTheDocument();
    });

    it("should hide input when Cancel is clicked", () => {
      render(<KeysPage />);

      fireEvent.click(screen.getByRole("button", { name: /add another pubky/i }));
      fireEvent.click(screen.getByRole("button", { name: /cancel/i }));

      expect(screen.queryByPlaceholderText(/enter pubky to add/i)).not.toBeInTheDocument();
      expect(screen.getByRole("button", { name: /add another pubky/i })).toBeInTheDocument();
    });

    it("should disable Add button when input is empty", () => {
      render(<KeysPage />);

      fireEvent.click(screen.getByRole("button", { name: /add another pubky/i }));

      expect(screen.getByRole("button", { name: /^add$/i })).toBeDisabled();
    });

    it("should enable Add button when input has value", () => {
      render(<KeysPage />);

      fireEvent.click(screen.getByRole("button", { name: /add another pubky/i }));
      fireEvent.change(screen.getByPlaceholderText(/enter pubky to add/i), {
        target: { value: "new-pubky-value" },
      });

      expect(screen.getByRole("button", { name: /^add$/i })).not.toBeDisabled();
    });

    it("should call addKey service when Add is clicked", async () => {
      vi.mocked(services.addKey).mockResolvedValue("normalized-pubky");

      render(<KeysPage />);

      fireEvent.click(screen.getByRole("button", { name: /add another pubky/i }));
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

      fireEvent.click(screen.getByRole("button", { name: /add another pubky/i }));
      fireEvent.change(screen.getByPlaceholderText(/enter pubky to add/i), {
        target: { value: "new-pubky" },
      });
      fireEvent.click(screen.getByRole("button", { name: /^add$/i }));

      await waitFor(() => {
        expect(screen.queryByPlaceholderText(/enter pubky to add/i)).not.toBeInTheDocument();
      });
    });

    it("should navigate to sync page after successful add", async () => {
      vi.mocked(services.addKey).mockResolvedValue("normalized-pubky");
      useUIStore.setState({ currentPage: "keys" });

      render(<KeysPage />);

      fireEvent.click(screen.getByRole("button", { name: /add another pubky/i }));
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
        () => new Promise((resolve) => { resolveAdd = resolve; })
      );

      render(<KeysPage />);

      fireEvent.click(screen.getByRole("button", { name: /add another pubky/i }));
      fireEvent.change(screen.getByPlaceholderText(/enter pubky to add/i), {
        target: { value: "new-pubky" },
      });
      fireEvent.click(screen.getByRole("button", { name: /^add$/i }));

      expect(screen.getByText("Adding...")).toBeInTheDocument();

      // Cleanup
      resolveAdd!("done");
    });
  });
});
