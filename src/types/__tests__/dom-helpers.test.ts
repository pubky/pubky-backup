import { describe, it, expect, beforeEach, afterEach } from "vitest";
import {
  getElementByIdStrict,
  getElementById,
  querySelectorStrict,
  querySelector,
} from "../dom-helpers";

describe("getElementByIdStrict", () => {
  beforeEach(() => {
    document.body.innerHTML = `
      <div id="test-element">Test Content</div>
      <input id="test-input" type="text" value="test value" />
      <button id="test-button">Click Me</button>
    `;
  });

  afterEach(() => {
    document.body.innerHTML = "";
  });

  it("should return element when it exists", () => {
    const element = getElementByIdStrict("test-element");
    expect(element).toBeDefined();
    expect(element.id).toBe("test-element");
    expect(element.textContent).toBe("Test Content");
  });

  it("should throw error when element not found", () => {
    expect(() => getElementByIdStrict("non-existent")).toThrow(
      "Element with id 'non-existent' not found",
    );
  });

  it("should return correct element type with generic - HTMLInputElement", () => {
    const input = getElementByIdStrict<HTMLInputElement>("test-input");
    expect(input.value).toBe("test value");
    expect(input.tagName).toBe("INPUT");
  });

  it("should return correct element type with generic - HTMLButtonElement", () => {
    const button = getElementByIdStrict<HTMLButtonElement>("test-button");
    expect(button.textContent).toBe("Click Me");
    expect(button.tagName).toBe("BUTTON");
  });

  it("should return correct element type with generic - HTMLDivElement", () => {
    const div = getElementByIdStrict<HTMLDivElement>("test-element");
    expect(div.textContent).toBe("Test Content");
    expect(div.tagName).toBe("DIV");
  });

  it("should throw error with specific message including the id", () => {
    expect(() => getElementByIdStrict("missing-element")).toThrow(
      "missing-element",
    );
  });
});

describe("getElementById", () => {
  beforeEach(() => {
    document.body.innerHTML = `
      <div id="test-element">Test Content</div>
      <input id="test-input" type="text" value="test value" />
    `;
  });

  afterEach(() => {
    document.body.innerHTML = "";
  });

  it("should return null when element not found", () => {
    const element = getElementById("non-existent");
    expect(element).toBeNull();
  });

  it("should return element when it exists", () => {
    const element = getElementById("test-element");
    expect(element).not.toBeNull();
    expect(element?.id).toBe("test-element");
    expect(element?.textContent).toBe("Test Content");
  });

  it("should return correct element type with generic - HTMLInputElement", () => {
    const input = getElementById<HTMLInputElement>("test-input");
    expect(input).not.toBeNull();
    expect(input?.value).toBe("test value");
    expect(input?.tagName).toBe("INPUT");
  });

  it("should return correct element type with generic - HTMLDivElement", () => {
    const div = getElementById<HTMLDivElement>("test-element");
    expect(div).not.toBeNull();
    expect(div?.textContent).toBe("Test Content");
    expect(div?.tagName).toBe("DIV");
  });

  it("should not throw error when element not found", () => {
    expect(() => getElementById("non-existent")).not.toThrow();
  });
});

describe("querySelectorStrict", () => {
  beforeEach(() => {
    document.body.innerHTML = `
      <div class="test-class">Test Content</div>
      <input class="input-class" type="text" value="test value" />
      <button data-testid="action-button">Click Me</button>
      <div id="unique-id">Unique Element</div>
    `;
  });

  afterEach(() => {
    document.body.innerHTML = "";
  });

  it("should return element when it exists - class selector", () => {
    const element = querySelectorStrict(".test-class");
    expect(element).toBeDefined();
    expect(element.textContent).toBe("Test Content");
  });

  it("should return element when it exists - attribute selector", () => {
    const element = querySelectorStrict('[data-testid="action-button"]');
    expect(element).toBeDefined();
    expect(element.textContent).toBe("Click Me");
  });

  it("should return element when it exists - id selector", () => {
    const element = querySelectorStrict("#unique-id");
    expect(element).toBeDefined();
    expect(element.textContent).toBe("Unique Element");
  });

  it("should throw error when element not found", () => {
    expect(() => querySelectorStrict(".non-existent")).toThrow(
      "Element matching selector '.non-existent' not found",
    );
  });

  it("should return correct element type with generic - HTMLInputElement", () => {
    const input = querySelectorStrict<HTMLInputElement>(".input-class");
    expect(input.value).toBe("test value");
    expect(input.tagName).toBe("INPUT");
  });

  it("should return correct element type with generic - HTMLButtonElement", () => {
    const button = querySelectorStrict<HTMLButtonElement>(
      '[data-testid="action-button"]',
    );
    expect(button.textContent).toBe("Click Me");
    expect(button.tagName).toBe("BUTTON");
  });

  it("should throw error with specific message including the selector", () => {
    expect(() => querySelectorStrict(".missing-class")).toThrow(
      ".missing-class",
    );
  });

  it("should work with complex selectors", () => {
    const element = querySelectorStrict("div.test-class");
    expect(element).toBeDefined();
    expect(element.textContent).toBe("Test Content");
  });
});

describe("querySelector", () => {
  beforeEach(() => {
    document.body.innerHTML = `
      <div class="test-class">Test Content</div>
      <input class="input-class" type="text" value="test value" />
      <button data-testid="action-button">Click Me</button>
    `;
  });

  afterEach(() => {
    document.body.innerHTML = "";
  });

  it("should return null when element not found", () => {
    const element = querySelector(".non-existent");
    expect(element).toBeNull();
  });

  it("should return element when it exists - class selector", () => {
    const element = querySelector(".test-class");
    expect(element).not.toBeNull();
    expect(element?.textContent).toBe("Test Content");
  });

  it("should return element when it exists - attribute selector", () => {
    const element = querySelector('[data-testid="action-button"]');
    expect(element).not.toBeNull();
    expect(element?.textContent).toBe("Click Me");
  });

  it("should return correct element type with generic - HTMLInputElement", () => {
    const input = querySelector<HTMLInputElement>(".input-class");
    expect(input).not.toBeNull();
    expect(input?.value).toBe("test value");
    expect(input?.tagName).toBe("INPUT");
  });

  it("should return correct element type with generic - HTMLButtonElement", () => {
    const button = querySelector<HTMLButtonElement>(
      '[data-testid="action-button"]',
    );
    expect(button).not.toBeNull();
    expect(button?.textContent).toBe("Click Me");
    expect(button?.tagName).toBe("BUTTON");
  });

  it("should not throw error when element not found", () => {
    expect(() => querySelector(".non-existent")).not.toThrow();
  });

  it("should work with complex selectors", () => {
    const element = querySelector("div.test-class");
    expect(element).not.toBeNull();
    expect(element?.textContent).toBe("Test Content");
  });
});
