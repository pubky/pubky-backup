import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import { QrCode } from "./QrCode";

describe("QrCode", () => {
  it("should render a code with its accessible label", () => {
    render(<QrCode value="pubkyauth://example" label="Sign-in QR code" />);

    expect(screen.getByTitle("Sign-in QR code")).toBeInTheDocument();
  });

  it("should encode different values differently", () => {
    const { container: first } = render(
      <QrCode value="pubkyauth://one" label="First" />,
    );
    const { container: second } = render(
      <QrCode value="pubkyauth://two" label="Second" />,
    );

    const modules = (container: HTMLElement) =>
      container.querySelectorAll("path")[1]?.getAttribute("d");
    expect(modules(first)).toBeTruthy();
    expect(modules(first)).not.toEqual(modules(second));
  });
});
