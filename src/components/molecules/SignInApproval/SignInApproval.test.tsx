import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { SignInApproval } from "./SignInApproval";

const URL = "pubkyauth://example";

describe("SignInApproval", () => {
  let app: HTMLElement;

  beforeEach(() => {
    // Stands in for the element the app is mounted on
    app = document.createElement("div");
    app.id = "root";
    document.body.appendChild(app);
  });

  afterEach(() => {
    app.remove();
  });

  it("should cancel on Escape", () => {
    const onCancel = vi.fn();
    render(
      <SignInApproval
        authorizationUrl={URL}
        title="Sign in"
        onCancel={onCancel}
      />,
    );

    fireEvent.keyDown(window, { key: "Escape" });

    expect(onCancel).toHaveBeenCalledOnce();
  });

  it("should ignore Escape while cancelling is disabled", () => {
    const onCancel = vi.fn();
    render(
      <SignInApproval
        authorizationUrl={URL}
        title="Sign in"
        onCancel={onCancel}
        cancelDisabled
      />,
    );

    fireEvent.keyDown(window, { key: "Escape" });

    expect(onCancel).not.toHaveBeenCalled();
  });

  it("should cancel only the topmost pop-up on Escape", () => {
    const onCancelFirst = vi.fn();
    const onCancelSecond = vi.fn();
    render(
      <>
        <SignInApproval
          authorizationUrl={URL}
          title="First"
          onCancel={onCancelFirst}
        />
        <SignInApproval
          authorizationUrl={URL}
          title="Second"
          onCancel={onCancelSecond}
        />
      </>,
    );

    fireEvent.keyDown(window, { key: "Escape" });

    expect(onCancelSecond).toHaveBeenCalledOnce();
    expect(onCancelFirst).not.toHaveBeenCalled();
  });

  it("should move focus into the pop-up", () => {
    render(
      <SignInApproval
        authorizationUrl={URL}
        title="Sign in"
        onCancel={vi.fn()}
      />,
    );

    expect(screen.getByRole("dialog", { name: "Sign in" })).toHaveFocus();
  });

  it("should make the app behind it inert until the last pop-up closes", () => {
    const first = render(
      <SignInApproval
        authorizationUrl={URL}
        title="First"
        onCancel={vi.fn()}
      />,
    );
    const second = render(
      <SignInApproval
        authorizationUrl={URL}
        title="Second"
        onCancel={vi.fn()}
      />,
    );
    expect(app).toHaveAttribute("inert");

    first.unmount();
    expect(app).toHaveAttribute("inert");

    second.unmount();
    expect(app).not.toHaveAttribute("inert");
  });
});
