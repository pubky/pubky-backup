import { useUIStore } from "@/stores/useUIStore";
import { StartupForm } from "@/components/organisms/StartupForm";
import { DashboardForm } from "@/components/organisms/DashboardForm";
import { Toast } from "@/components/molecules/Toast";

export function App() {
  const currentScreen = useUIStore((s) => s.currentScreen);

  return (
    <>
      {currentScreen === "startup" ? <StartupForm /> : <DashboardForm />}
      <Toast />
    </>
  );
}
