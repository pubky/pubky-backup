import * as Stores from "@/stores";
import * as Organisms from "@/components/organisms";
import * as Molecules from "@/components/molecules";

export function App() {
  const currentScreen = Stores.useUIStore((s) => s.currentScreen);

  return (
    <>
      {currentScreen === "startup" ? <Organisms.StartupForm /> : <Organisms.DashboardForm />}
      <Molecules.Toast />
    </>
  );
}
