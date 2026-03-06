import { useEffect } from "react";
import * as Stores from "@/stores";
import * as Atoms from "@/components/atoms";
import * as Organisms from "@/components/organisms";
import * as Molecules from "@/components/molecules";
import * as Hooks from "@/hooks";
import { getAllKeyStates, getConfig, getLastPubky } from "@/services";
import { Logger } from "@/lib";
import { stripPubkyPrefix } from "@/utils/pubky";

/**
 * Bootstrap the app by fetching initial state from backend.
 * Sets up event listener first, then fetches initial state to avoid race conditions.
 */
function useBootstrap() {
  const { setAllKeyStates, setDeveloperMode, setViewedPubky } =
    Stores.useUIStore.getState();

  // Set up event listener for key updates (must be first to avoid race)
  Hooks.useKeyUpdates();

  // Bootstrap on mount
  useEffect(() => {
    const bootstrap = async () => {
      try {
        // Fetch config (one-time)
        const config = await getConfig();
        setDeveloperMode(config.developer_mode);

        // Fetch all key states
        const keyStates = await getAllKeyStates();
        setAllKeyStates(keyStates);

        // Restore last viewed pubky (normalize to strip any "pubky" prefix)
        const lastPubky = await getLastPubky();
        if (lastPubky !== null) {
          setViewedPubky(stripPubkyPrefix(lastPubky));
        }
      } catch (error) {
        Logger.error("App", "Failed to bootstrap", { error });
      }
    };

    void bootstrap();
  }, [setAllKeyStates, setDeveloperMode, setViewedPubky]);
}

function DashboardPageContent() {
  const currentPage = Stores.useUIStore((s) => s.currentPage);

  switch (currentPage) {
    case "sync":
      return <Organisms.DashboardForm />;
    case "activity":
      return <Organisms.ActivityPage />;
    case "keys":
      return <Organisms.KeysPage />;
    case "settings":
      return <Organisms.SettingsPage />;
  }
}

export function App() {
  const currentScreen = Stores.useUIStore((s) => s.currentScreen);

  // Bootstrap app state from backend
  useBootstrap();

  return (
    <>
      {currentScreen === "startup" ? (
        <Organisms.StartupForm />
      ) : (
        <main className="flex flex-col items-center justify-start text-center relative">
          <Atoms.Card className="justify-start">
            <div className="flex justify-end">
              <Molecules.NavBar />
            </div>
            <DashboardPageContent />
          </Atoms.Card>
        </main>
      )}
      <Molecules.Toast />
    </>
  );
}
