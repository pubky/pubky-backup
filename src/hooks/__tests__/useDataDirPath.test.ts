import { describe, it, expect, vi, beforeEach } from "vitest";
import { renderHook, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { createElement, type ReactNode } from "react";
import { useDataDirPath } from "../useDataDirPath";
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

describe("useDataDirPath", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("should fetch data directory path when enabled", async () => {
    const mockPath = "/home/user/.local/share/pubky-backup";
    vi.mocked(tauriCommands.getDataDirPath).mockResolvedValue(mockPath);

    const { result } = renderHook(() => useDataDirPath(true), {
      wrapper: createWrapper(),
    });

    await waitFor(() => {
      expect(result.current.isSuccess).toBe(true);
    });

    expect(result.current.data).toBe(mockPath);
    expect(tauriCommands.getDataDirPath).toHaveBeenCalled();
  });

  it("should be enabled by default", async () => {
    const mockPath = "/default/path";
    vi.mocked(tauriCommands.getDataDirPath).mockResolvedValue(mockPath);

    const { result } = renderHook(() => useDataDirPath(), {
      wrapper: createWrapper(),
    });

    await waitFor(() => {
      expect(result.current.isSuccess).toBe(true);
    });

    expect(tauriCommands.getDataDirPath).toHaveBeenCalled();
  });

  it("should not fetch when disabled", async () => {
    const { result } = renderHook(() => useDataDirPath(false), {
      wrapper: createWrapper(),
    });

    // Wait a tick to ensure no fetch happens
    await new Promise((resolve) => setTimeout(resolve, 50));

    expect(result.current.isFetching).toBe(false);
    expect(tauriCommands.getDataDirPath).not.toHaveBeenCalled();
  });

  it("should handle errors", async () => {
    const mockError = { type: "Storage", message: "Path not found" };
    vi.mocked(tauriCommands.getDataDirPath).mockRejectedValue(mockError);

    const { result } = renderHook(() => useDataDirPath(true), {
      wrapper: createWrapper(),
    });

    await waitFor(() => {
      expect(result.current.isError).toBe(true);
    });

    expect(result.current.error).toEqual(mockError);
  });
});
