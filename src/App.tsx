import * as Stores from "@/stores";
import * as Atoms from "@/components/atoms";
import * as Organisms from "@/components/organisms";
import * as Molecules from "@/components/molecules";

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

  return (
    <>
      {currentScreen === "startup" ? (
        <Organisms.StartupForm />
      ) : (
        <main className="flex flex-col items-center justify-start text-center relative">
          <Atoms.Card>
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
