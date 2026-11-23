import { describe, it, expect, beforeEach, vi } from "vitest";
import {
  render,
  screen,
  fireEvent,
  waitFor,
  act,
  within,
} from "@testing-library/react";
import { MirrorCard } from "./MirrorCard";
import { useUIStore } from "@/stores/uiStore";
import type { KeyState, MirrorStatus } from "@/stores/uiStore";
import * as services from "@/services";
import { displayPubky } from "@/utils";

vi.mock("@/services", () => ({
  getConfig: vi.fn(),
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

/** Render the card, waiting for it to learn whether mirroring is offered. */
async function renderCard() {
  const view = render(<MirrorCard pubky={PUBKY} />);
  await act(async () => {});
  return view;
}

async function renderWithMirror(
  mirror: MirrorStatus = { type: "NotMirrored" },
) {
  const keyState: KeyState = {
    status: { type: "Idle" },
    data_size: 1024,
    last_sync: null,
    next_sync: null,
    error: null,
    auth: { type: "SignedOut" },
    mirror,
  };
  useUIStore.setState({ keyStates: { [PUBKY]: keyState } });
  return renderCard();
}

describe("MirrorCard", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(services.getConfig).mockResolvedValue({
      sync_interval_secs: 300,
      keys_dir: "/home/user/.pubky-backup/keys",
      mirroring: true,
    });
    useUIStore.setState({
      keyStates: {},
      toast: { visible: false, pubkyText: "", type: "success", message: "" },
    });
  });

  it("should leave the mirror out when this build doesn't offer it", async () => {
    vi.mocked(services.getConfig).mockResolvedValue({
      sync_interval_secs: 300,
      keys_dir: "/home/user/.pubky-backup/keys",
      mirroring: false,
    });
    const { container } = await renderWithMirror();

    expect(container).toBeEmptyDOMElement();
  });

  it("should offer to add a mirror when there is none", async () => {
    await renderWithMirror();

    expect(screen.getByText("Mirror")).toBeInTheDocument();
    expect(screen.getByText(/Not mirrored/i)).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "Add mirror" }),
    ).toBeInTheDocument();
  });

  it("should ask for the homeserver and request approval", async () => {
    vi.mocked(services.startMirror).mockResolvedValue(AUTHORIZATION_URL);
    await renderWithMirror();

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
    await renderWithMirror();
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

  it("should close the form on cancel", async () => {
    await renderWithMirror();
    fireEvent.click(screen.getByRole("button", { name: "Add mirror" }));

    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));

    expect(
      screen.getByRole("button", { name: "Add mirror" }),
    ).toBeInTheDocument();
  });

  it("should show the QR code while the mirror awaits approval", async () => {
    vi.mocked(services.cancelMirror).mockResolvedValue(undefined);
    await renderWithMirror({
      type: "AwaitingApproval",
      homeserver: HOMESERVER,
      authorization_url: AUTHORIZATION_URL,
    });

    const dialog = within(
      screen.getByRole("dialog", { name: "Approve the mirror" }),
    );
    expect(dialog.getByTitle("Sign-in QR code")).toBeInTheDocument();
    expect(dialog.getByText(/grants write access/i)).toBeInTheDocument();

    fireEvent.click(dialog.getByRole("button", { name: "Cancel" }));
    await waitFor(() => {
      expect(services.cancelMirror).toHaveBeenCalledWith(PUBKY);
    });
  });

  it("should show a working mirror and let the user stop it", async () => {
    vi.mocked(services.stopMirror).mockResolvedValue(undefined);
    await renderWithMirror({
      type: "Mirroring",
      homeserver: HOMESERVER,
    });

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
    await renderWithMirror({
      type: "SessionExpired",
      homeserver: HOMESERVER,
    });

    expect(screen.getByText(/Paused:/i)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Approve again" }));

    await waitFor(() => {
      expect(services.startMirror).toHaveBeenCalledWith(PUBKY, HOMESERVER);
    });
  });

  it("should let the user stop a paused mirror instead of approving it again", async () => {
    vi.mocked(services.stopMirror).mockResolvedValue(undefined);
    await renderWithMirror({
      type: "SessionExpired",
      homeserver: HOMESERVER,
    });

    fireEvent.click(screen.getByRole("button", { name: "Stop" }));

    await waitFor(() => {
      expect(services.stopMirror).toHaveBeenCalledWith(PUBKY);
    });
    expect(services.startMirror).not.toHaveBeenCalled();
  });

  it("should show why copying is failing", async () => {
    await renderWithMirror({
      type: "Failing",
      homeserver: HOMESERVER,
      message: "The mirror responded with 507 Insufficient Storage",
    });

    expect(
      screen.getByText(/is failing: The mirror responded with 507/i),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Stop" })).toBeInTheDocument();
  });

  it("should reopen the form with the homeserver after a failed setup", async () => {
    await renderWithMirror({
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
