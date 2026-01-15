import { describe, it, expect, vi, beforeEach } from "vitest";
import { renderHook, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { createElement, type ReactNode } from "react";
import { useLastPubky } from "../useLastPubky";
import * as tauriCommands from "@/services/tauri-commands";

vi.mock("@/services/tauri-commands");

function createWrapper() {
  const queryClient = new QueryClient({
    defaultOptions: {
      queries: {
        retry: false,
      },
    },
  });
  return function Wrapper({ children }: { children: ReactNode }) {
    return createElement(QueryClientProvider, { client: queryClient }, children);
  };
}

describe("useLastPubky", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("should fetch last used pubky", async () => {
    const mockPubky = "pk:lastused123";
    vi.mocked(tauriCommands.getLastPubky).mockResolvedValue(mockPubky);

    const { result } = renderHook(() => useLastPubky(), {
      wrapper: createWrapper(),
    });

    await waitFor(() => {
      expect(result.current.isSuccess).toBe(true);
    });

    expect(result.current.data).toBe(mockPubky);
    expect(tauriCommands.getLastPubky).toHaveBeenCalled();
  });

  it("should handle null (no previous pubky)", async () => {
    vi.mocked(tauriCommands.getLastPubky).mockResolvedValue(null);

    const { result } = renderHook(() => useLastPubky(), {
      wrapper: createWrapper(),
    });

    await waitFor(() => {
      expect(result.current.isSuccess).toBe(true);
    });

    expect(result.current.data).toBeNull();
  });

  it("should handle errors", async () => {
    const mockError = { type: "Storage", message: "Read failed" };
    vi.mocked(tauriCommands.getLastPubky).mockRejectedValue(mockError);

    const { result } = renderHook(() => useLastPubky(), {
      wrapper: createWrapper(),
    });

    await waitFor(() => {
      expect(result.current.isError).toBe(true);
    });

    expect(result.current.error).toEqual(mockError);
  });
});
