/**
 * Checks the wrappers in tauri-commands.ts against the commands the Tauri
 * shell defines, by reading both sources. Command names and arguments are
 * mirrored by hand, so a mismatch would otherwise only fail in the running app.
 */
import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

// Vitest runs from the repository root
const shellSource = readFileSync(
  resolve(process.cwd(), "src-tauri/src/lib.rs"),
  "utf8",
);
const wrapperSource = readFileSync(
  resolve(process.cwd(), "src/services/tauri-commands.ts"),
  "utf8",
);

/** Parameter types Tauri provides itself, which the frontend never sends */
const INJECTED_TYPES = ["AppHandle", "State<", "Window", "WebviewWindow"];

interface CommandArgument {
  /** The key the frontend sends it under: Tauri expects camelCase */
  key: string;
  isOptional: boolean;
}

function toCamelCase(snakeCase: string): string {
  return snakeCase.replace(/_([a-z])/g, (_, letter: string) =>
    letter.toUpperCase(),
  );
}

/** Split a parameter list on the commas that are not inside generics. */
function splitParameters(list: string): string[] {
  const parameters: string[] = [];
  let depth = 0;
  let current = "";
  for (const char of list) {
    if (char === "<") depth++;
    if (char === ">") depth--;
    if (char === "," && depth === 0) {
      parameters.push(current);
      current = "";
    } else {
      current += char;
    }
  }
  parameters.push(current);
  return parameters.map((p) => p.trim()).filter((p) => p.length > 0);
}

/** The `#[tauri::command]` functions in the shell, with what the frontend sends them. */
function definedCommands(): Map<string, CommandArgument[]> {
  const commands = new Map<string, CommandArgument[]>();
  const pattern =
    /#\[tauri::command\]\s*(?:pub\s+)?(?:async\s+)?fn\s+(\w+)\s*\(([^)]*)\)/g;
  for (const [, name, list] of shellSource.matchAll(pattern)) {
    const sent = splitParameters(list ?? "")
      .map((parameter) => {
        const [argument = "", type = ""] = parameter
          .split(/:(.*)/s)
          .map((s) => s.trim());
        return { argument, type };
      })
      .filter(({ type }) => !INJECTED_TYPES.some((t) => type.startsWith(t)))
      .map(({ argument, type }) => ({
        key: toCamelCase(argument),
        isOptional: type.startsWith("Option<"),
      }));
    commands.set(name ?? "", sent);
  }
  return commands;
}

/** The commands listed in `generate_handler!`. */
function registeredCommands(): string[] {
  const list = /generate_handler!\[([^\]]*)\]/.exec(shellSource)?.[1] ?? "";
  return list
    .split(",")
    .map((name) => name.trim())
    .filter((name) => name.length > 0);
}

/** Each `invoke` in the wrappers: the command and the keys of its arguments. */
function invokedCommands(): Map<string, string[]> {
  const invocations = new Map<string, string[]>();
  const pattern = /invoke(?:<.*?>)?\(\s*"(\w+)"\s*(?:,\s*\{([^}]*)\})?/gs;
  for (const [, name, object] of wrapperSource.matchAll(pattern)) {
    const keys = (object ?? "")
      .split(",")
      .map((entry) => entry.split(":")[0]?.trim() ?? "")
      .filter((key) => key.length > 0);
    invocations.set(name ?? "", keys);
  }
  return invocations;
}

describe("Tauri command wrappers", () => {
  it("should find the commands and wrappers to compare", () => {
    expect(definedCommands().size).toBeGreaterThan(0);
    expect(invokedCommands().size).toBeGreaterThan(0);
  });

  it("should register every command the shell defines, and only those", () => {
    expect(registeredCommands().sort()).toEqual(
      [...definedCommands().keys()].sort(),
    );
  });

  it("should invoke exactly the registered commands", () => {
    expect([...invokedCommands().keys()].sort()).toEqual(
      registeredCommands().sort(),
    );
  });

  it("should send each command exactly the arguments it takes", () => {
    const defined = definedCommands();
    for (const [name, keys] of invokedCommands()) {
      const expected = defined.get(name) ?? [];
      const accepted = expected.map((argument) => argument.key);
      const required = expected
        .filter((argument) => !argument.isOptional)
        .map((argument) => argument.key);

      expect({
        name,
        unknown: keys.filter((k) => !accepted.includes(k)),
      }).toEqual({ name, unknown: [] });
      expect({
        name,
        missing: required.filter((k) => !keys.includes(k)),
      }).toEqual({ name, missing: [] });
    }
  });
});
