import { describe, it, expect, beforeEach, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import { ActivityPage } from "./ActivityPage";
import { useUIStore } from "@/stores/uiStore";
import type { ActivityEntry } from "@/stores/uiStore";
import * as commands from "@/services/tauri-commands";

vi.mock("@/services/tauri-commands", () => ({
  getActivity: vi.fn(),
  openSnapshotsDir: vi.fn(),
}));

/** The coloured dot shown in front of an activity entry's message. */
function dotOf(message: RegExp): Element | null {
  return screen.getByText(message).previousElementSibling;
}

describe("ActivityPage", () => {
  const now = Math.floor(Date.now() / 1000);

  beforeEach(() => {
    vi.clearAllMocks();
    useUIStore.setState({ lastPubky: "test-pubky", keyStates: {} });
  });

  it("should show the activity of the selected pubky", async () => {
    const entries: ActivityEntry[] = [
      { type: "signed_in", message: "Signed in", timestamp: now },
    ];
    vi.mocked(commands.getActivity).mockResolvedValue(entries);

    render(<ActivityPage />);

    expect(await screen.findByText("Signed in")).toBeInTheDocument();
    expect(commands.getActivity).toHaveBeenCalledWith("test-pubky");
  });

  it("should mark lost coverage as a failure and other activity as fine", async () => {
    const entries: ActivityEntry[] = [
      { type: "session_expired", message: "Session expired", timestamp: now },
      { type: "signed_out", message: "Signed out", timestamp: now },
      { type: "files_backed_up", message: "2 new files", timestamp: now },
    ];
    vi.mocked(commands.getActivity).mockResolvedValue(entries);

    render(<ActivityPage />);

    await screen.findByText("Session expired");
    expect(dotOf(/Session expired/)).toHaveClass("bg-red-500");
    expect(dotOf(/Signed out/)).toHaveClass("bg-green-500");
    expect(dotOf(/2 new files/)).toHaveClass("bg-green-500");
  });

  it("should show a placeholder when there is no activity", async () => {
    vi.mocked(commands.getActivity).mockResolvedValue([]);

    render(<ActivityPage />);

    expect(await screen.findByText("No activity yet")).toBeInTheDocument();
  });
});
