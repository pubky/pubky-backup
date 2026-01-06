import { GreetForm } from "@/greet-form";
import { MainForm } from "@/main-form";
import { getElementByIdStrict } from "@/types/dom-helpers";

function showScreen(screenId: string): void {
  document.querySelectorAll(".screen").forEach((screen) => {
    screen.classList.add("hidden");
  });
  getElementByIdStrict<HTMLElement>(screenId).classList.remove("hidden");
}

function init(): void {
  // Initialize the pubky form (startup screen)
  const greetForm = new GreetForm(() => {
    showScreen("app-screen");
    initMainForm();
  });
  void greetForm.init();
}

function initMainForm(): void {
  // Initialize the main app form
  const mainForm = new MainForm();
  mainForm.init();
}

init();
