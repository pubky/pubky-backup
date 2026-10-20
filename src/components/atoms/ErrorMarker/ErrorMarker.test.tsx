import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import { ErrorMarker } from "./ErrorMarker";

describe("ErrorMarker", () => {
  it("should say the backup failed", () => {
    render(<ErrorMarker />);
    expect(screen.getByTitle("Backup failed")).toBeInTheDocument();
  });

  it("should render the icon in red at the given size", () => {
    const { container } = render(<ErrorMarker size={12} />);
    const svg = container.querySelector("svg");
    expect(svg).toHaveClass("text-pubky-red");
    expect(svg).toHaveAttribute("width", "12");
  });
});
