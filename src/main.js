import { GreetForm } from "./greet-form.js";
import { MainForm } from "./main-form.js";

function showScreen(screenId) {
  document.querySelectorAll(".screen").forEach((screen) => {
    screen.classList.add("hidden");
  });
  document.getElementById(screenId).classList.remove("hidden");
}

function init() {
  // Initialize the pubky form (startup screen)
  const greetForm = new GreetForm(() => {
    showScreen("app-screen");
    initMainForm();
  });
  greetForm.init();
}

function initMainForm() {
  // Initialize the main app form
  const mainForm = new MainForm();
  mainForm.init();
}

init();
