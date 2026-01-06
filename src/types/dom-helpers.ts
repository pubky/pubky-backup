/**
 * Type-safe DOM query utilities to prevent null reference errors
 */

/**
 * Get an element by ID with strict null checking
 * Throws an error if element is not found
 * @param id - The element ID to query
 * @returns The element (guaranteed to exist)
 * @throws {Error} If element is not found
 */
export function getElementByIdStrict<T extends HTMLElement = HTMLElement>(
  id: string,
): T {
  const element = document.getElementById(id);
  if (element === null) {
    throw new Error(`Element with id '${id}' not found`);
  }
  return element as T;
}

/**
 * Get an element by ID with nullable return type
 * Returns null if element is not found
 * @param id - The element ID to query
 * @returns The element or null if not found
 */
export function getElementById<T extends HTMLElement = HTMLElement>(
  id: string,
): T | null {
  const element = document.getElementById(id);
  return element as T | null;
}

/**
 * Query selector with strict null checking
 * Throws an error if element is not found
 * @param selector - The CSS selector to query
 * @returns The element (guaranteed to exist)
 * @throws {Error} If element is not found
 */
export function querySelectorStrict<T extends Element = Element>(
  selector: string,
): T {
  const element = document.querySelector<T>(selector);
  if (element === null) {
    throw new Error(`Element matching selector '${selector}' not found`);
  }
  return element;
}

/**
 * Query selector with nullable return type
 * Returns null if element is not found
 * @param selector - The CSS selector to query
 * @returns The element or null if not found
 */
export function querySelector<T extends Element = Element>(
  selector: string,
): T | null {
  return document.querySelector<T>(selector);
}
