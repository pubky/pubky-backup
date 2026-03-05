import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { createElement, type ReactNode } from "react";
import { KeysPage } from "./KeysPage";
import { useUIStore } from "@/stores";
import * as services from "@/services";

vi.mock("@/services");

// Mock clipboard API
const mockWriteText = vi.fn();
Object.defineProperty(navigator, "clipboard", {
  value: {
    writeText: mockWriteText,
  },
  writable: true,
  configurable: true,
});

function createWrapper() {
  const queryClient = new QueryClient({
    defaultOptions: {
      queries: {
        retry: false,
      },
    },
  });
  return function Wrapper({ children }: { children: ReactNode }) {
    return createElement(
      QueryClientProvider,
      { client: queryClient },
      children,
    );
  };
}

describe("KeysPage", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockWriteText.mockResolvedValue(undefined);
    // Reset store state
    useUIStore.setState({
      currentPage: "keys",
      toast: { visible: false, pubkyText: "", type: "success", message: "" },
    });
    // Default mocks
    vi.mocked(services.getKeys).mockResolvedValue([]);
    vi.mocked(services.fetchState).mockResolvedValue({
      pubky: null,
      developerMode: false,
      isSyncing: false,
      nextSyncTime: 0,
      dataDirSize: 0,
      backupControllerError: null,
      backupRunning: false,
    });
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("should render the keys page header", () => {
    render(<KeysPage />, { wrapper: createWrapper() });

    expect(screen.getByText("Manage keys")).toBeInTheDocument();
    expect(
      screen.getByText("Select a pubky to see its backup activity and status."),
    ).toBeInTheDocument();
  });

  it("should display the keys count", async () => {
    vi.mocked(services.getKeys).mockResolvedValue(["pk:key1", "pk:key2"]);

    render(<KeysPage />, { wrapper: createWrapper() });

    await waitFor(() => {
      expect(screen.getByText("Your pubkys (2)")).toBeInTheDocument();
    });
  });

  it("should display empty keys count when no keys", async () => {
    vi.mocked(services.getKeys).mockResolvedValue([]);

    render(<KeysPage />, { wrapper: createWrapper() });

    await waitFor(() => {
      expect(screen.getByText("Your pubkys (0)")).toBeInTheDocument();
    });
  });

  it("should render key items for each key", async () => {
    vi.mocked(services.getKeys).mockResolvedValue([
      "pk:abc123456789",
      "pk:def987654321",
    ]);

    render(<KeysPage />, { wrapper: createWrapper() });

    await waitFor(() => {
      // Keys should be displayed
      expect(screen.getByText("Your pubkys (2)")).toBeInTheDocument();
      // Copy buttons for each key
      expect(screen.getAllByTitle("Copy pubky")).toHaveLength(2);
    });
  });

  it("should show 'Add another pubky' button by default", () => {
    render(<KeysPage />, { wrapper: createWrapper() });

    expect(
      screen.getByRole("button", { name: /add another pubky/i }),
    ).toBeInTheDocument();
  });

  it("should show add input when 'Add another pubky' is clicked", async () => {
    render(<KeysPage />, { wrapper: createWrapper() });

    const addButton = screen.getByRole("button", { name: /add another pubky/i });
    fireEvent.click(addButton);

    await waitFor(() => {
      expect(
        screen.getByPlaceholderText("Enter pubky to add..."),
      ).toBeInTheDocument();
      expect(screen.getByRole("button", { name: /cancel/i })).toBeInTheDocument();
      expect(screen.getByRole("button", { name: /add$/i })).toBeInTheDocument();
    });
  });

  it("should hide add input when Cancel is clicked", async () => {
    render(<KeysPage />, { wrapper: createWrapper() });

    // Show add input
    const addButton = screen.getByRole("button", { name: /add another pubky/i });
    fireEvent.click(addButton);

    // Click cancel
    const cancelButton = screen.getByRole("button", { name: /cancel/i });
    fireEvent.click(cancelButton);

    await waitFor(() => {
      expect(
        screen.queryByPlaceholderText("Enter pubky to add..."),
      ).not.toBeInTheDocument();
      expect(
        screen.getByRole("button", { name: /add another pubky/i }),
      ).toBeInTheDocument();
    });
  });

  it("should disable Add button when input is empty", async () => {
    render(<KeysPage />, { wrapper: createWrapper() });

    // Show add input
    const addButton = screen.getByRole("button", { name: /add another pubky/i });
    fireEvent.click(addButton);

    await waitFor(() => {
      const submitButton = screen.getByRole("button", { name: /^add$/i });
      expect(submitButton).toBeDisabled();
    });
  });

  it("should call addKey when submitting new pubky", async () => {
    vi.mocked(services.addKey).mockResolvedValue(undefined);

    render(<KeysPage />, { wrapper: createWrapper() });

    // Show add input
    const addAnotherButton = screen.getByRole("button", {
      name: /add another pubky/i,
    });
    fireEvent.click(addAnotherButton);

    // Type pubky
    const input = screen.getByPlaceholderText("Enter pubky to add...");
    fireEvent.change(input, { target: { value: "pk:newkey123" } });

    // Submit
    const addButton = screen.getByRole("button", { name: /^add$/i });
    fireEvent.click(addButton);

    await waitFor(() => {
      expect(services.addKey).toHaveBeenCalledWith("pk:newkey123");
    });
  });

  it("should navigate to sync page after adding key", async () => {
    vi.mocked(services.addKey).mockResolvedValue(undefined);

    render(<KeysPage />, { wrapper: createWrapper() });

    // Show add input and submit
    fireEvent.click(screen.getByRole("button", { name: /add another pubky/i }));
    fireEvent.change(screen.getByPlaceholderText("Enter pubky to add..."), {
      target: { value: "pk:newkey123" },
    });
    fireEvent.click(screen.getByRole("button", { name: /^add$/i }));

    await waitFor(() => {
      expect(useUIStore.getState().currentPage).toBe("sync");
    });
  });

  it("should show error toast on addKey failure", async () => {
    const mockError = { type: "InvalidPubkyFormat", message: "Bad format" };
    vi.mocked(services.addKey).mockRejectedValue(mockError);

    render(<KeysPage />, { wrapper: createWrapper() });

    // Show add input and submit
    fireEvent.click(screen.getByRole("button", { name: /add another pubky/i }));
    fireEvent.change(screen.getByPlaceholderText("Enter pubky to add..."), {
      target: { value: "bad-pubky" },
    });
    fireEvent.click(screen.getByRole("button", { name: /^add$/i }));

    await waitFor(() => {
      const toast = useUIStore.getState().toast;
      expect(toast.visible).toBe(true);
      expect(toast.type).toBe("error");
    });
  });

  it("should trim whitespace from pubky input before adding", async () => {
    vi.mocked(services.addKey).mockResolvedValue(undefined);

    render(<KeysPage />, { wrapper: createWrapper() });

    // Show add input and submit with whitespace
    fireEvent.click(screen.getByRole("button", { name: /add another pubky/i }));
    fireEvent.change(screen.getByPlaceholderText("Enter pubky to add..."), {
      target: { value: "  pk:newkey123  " },
    });
    fireEvent.click(screen.getByRole("button", { name: /^add$/i }));

    await waitFor(() => {
      expect(services.addKey).toHaveBeenCalledWith("pk:newkey123");
    });
  });

  describe("key selection", () => {
    it("should navigate to sync page when clicking current key", async () => {
      vi.mocked(services.getKeys).mockResolvedValue(["pk:currentkey"]);
      vi.mocked(services.fetchState).mockResolvedValue({
        pubky: "pk:currentkey",
        developerMode: false,
        isSyncing: false,
        nextSyncTime: 0,
        dataDirSize: 0,
        backupControllerError: null,
        backupRunning: false,
      });

      render(<KeysPage />, { wrapper: createWrapper() });

      await waitFor(() => {
        expect(screen.getByText("Your pubkys (1)")).toBeInTheDocument();
      });

      // Click the current key
      const keyButtons = screen.getAllByRole("button");
      const keyButton = keyButtons[0]; // First button is the key item
      fireEvent.click(keyButton);

      await waitFor(() => {
        expect(useUIStore.getState().currentPage).toBe("sync");
      });

      // Should not have called setViewedPubky since it's already selected
      expect(services.setViewedPubky).not.toHaveBeenCalled();
    });

    it("should call setViewedPubky when clicking different key", async () => {
      vi.mocked(services.getKeys).mockResolvedValue([
        "pk:currentkey",
        "pk:otherkey",
      ]);
      vi.mocked(services.fetchState).mockResolvedValue({
        pubky: "pk:currentkey",
        developerMode: false,
        isSyncing: false,
        nextSyncTime: 0,
        dataDirSize: 0,
        backupControllerError: null,
        backupRunning: false,
      });
      vi.mocked(services.setViewedPubky).mockResolvedValue(undefined);

      render(<KeysPage />, { wrapper: createWrapper() });

      await waitFor(() => {
        expect(screen.getByText("Your pubkys (2)")).toBeInTheDocument();
      });

      // Click the other key by finding its text
      const otherKeyText = screen.getByText("pk:otherkey");
      // Click the parent button
      fireEvent.click(otherKeyText.closest("button")!);

      await waitFor(() => {
        expect(services.setViewedPubky).toHaveBeenCalled();
        expect(vi.mocked(services.setViewedPubky).mock.calls[0][0]).toBe(
          "pk:otherkey",
        );
      });
    });

    it("should show error toast on setViewedPubky failure", async () => {
      vi.mocked(services.getKeys).mockResolvedValue([
        "pk:currentkey",
        "pk:otherkey",
      ]);
      vi.mocked(services.fetchState).mockResolvedValue({
        pubky: "pk:currentkey",
        developerMode: false,
        isSyncing: false,
        nextSyncTime: 0,
        dataDirSize: 0,
        backupControllerError: null,
        backupRunning: false,
      });
      const mockError = { type: "Internal", message: "Failed to switch" };
      vi.mocked(services.setViewedPubky).mockRejectedValue(mockError);

      render(<KeysPage />, { wrapper: createWrapper() });

      await waitFor(() => {
        expect(screen.getByText("Your pubkys (2)")).toBeInTheDocument();
      });

      // Click the other key
      const otherKeyText = screen.getByText("pk:otherkey");
      fireEvent.click(otherKeyText.closest("button")!);

      await waitFor(() => {
        const toast = useUIStore.getState().toast;
        expect(toast.visible).toBe(true);
        expect(toast.type).toBe("error");
      });
    });
  });

  describe("copy functionality", () => {
    it("should copy pubky to clipboard when copy button is clicked", async () => {
      vi.mocked(services.getKeys).mockResolvedValue(["pk:copyablekey"]);

      render(<KeysPage />, { wrapper: createWrapper() });

      await waitFor(() => {
        expect(screen.getByText("Your pubkys (1)")).toBeInTheDocument();
      });

      // Find and click the copy button
      const copyButton = screen.getByTitle("Copy pubky");
      fireEvent.click(copyButton);

      await waitFor(() => {
        expect(mockWriteText).toHaveBeenCalledWith("pk:copyablekey");
      });
    });

    it("should show toast after copying", async () => {
      vi.mocked(services.getKeys).mockResolvedValue(["pk:copyablekey"]);

      render(<KeysPage />, { wrapper: createWrapper() });

      await waitFor(() => {
        expect(screen.getByText("Your pubkys (1)")).toBeInTheDocument();
      });

      const copyButton = screen.getByTitle("Copy pubky");
      fireEvent.click(copyButton);

      await waitFor(() => {
        const toast = useUIStore.getState().toast;
        expect(toast.visible).toBe(true);
        expect(toast.pubkyText).toBe("pk:copyablekey");
      });
    });
  });
});
