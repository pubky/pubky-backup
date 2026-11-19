import { describe, it, expect, beforeEach, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import { ActivityPage } from "./ActivityPage";
import { useUIStore } from "@/stores/uiStore";
import type { ActivityType, StackedActivity } from "@/stores/uiStore";
import * as commands from "@/services/tauri-commands";

vi.mock("@/services/tauri-commands", () => ({
  getActivity: vi.fn(),
  openSnapshotsDir: vi.fn(),
}));

/** An entry that occurred once, at `timestamp`. */
function once(
  type: ActivityType,
  message: string,
  timestamp: number,
): StackedActivity {
  return { type, message, timestamp, count: 1 };
}

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
    const entries: StackedActivity[] = [once("signed_in", "Signed in", now)];
    vi.mocked(commands.getActivity).mockResolvedValue(entries);

    render(<ActivityPage />);

    expect(await screen.findByText("Signed in")).toBeInTheDocument();
    expect(commands.getActivity).toHaveBeenCalledWith("test-pubky");
  });

  it("should mark lost coverage as a failure and other activity as fine", async () => {
    const entries: StackedActivity[] = [
      once("session_expired", "Session expired", now),
      once("signed_out", "Signed out", now),
      once("files_backed_up", "2 changes backed up", now),
    ];
    vi.mocked(commands.getActivity).mockResolvedValue(entries);

    render(<ActivityPage />);

    await screen.findByText("Session expired");
    expect(dotOf(/Session expired/)).toHaveClass("bg-red-500");
    expect(dotOf(/Signed out/)).toHaveClass("bg-green-500");
    expect(dotOf(/2 changes backed up/)).toHaveClass("bg-green-500");
  });

  it("should show how often a stacked entry occurred", async () => {
    const entries: StackedActivity[] = [
      {
        type: "sync_failed",
        message: "Offline",
        timestamp: now,
        count: 37,
      },
      once("signed_in", "Signed in", now - 4 * 3600),
    ];
    vi.mocked(commands.getActivity).mockResolvedValue(entries);

    render(<ActivityPage />);

    expect(await screen.findByText("Just now · 37 times")).toBeInTheDocument();
    expect(screen.getByText("4 hours ago")).toBeInTheDocument();
  });

  it("should show a placeholder when there is no activity", async () => {
    vi.mocked(commands.getActivity).mockResolvedValue([]);

    render(<ActivityPage />);

    expect(await screen.findByText("No activity yet")).toBeInTheDocument();
  });
});
