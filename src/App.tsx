import { useEffect } from "react";
import { AppShell } from "@/layouts/AppShell";
import { AboutPage } from "@/pages/AboutPage";
import { AppsPage } from "@/pages/AppsPage";
import { DashboardPage } from "@/pages/DashboardPage";
import { DevicesPage } from "@/pages/DevicesPage";
import { MirrorPage } from "@/pages/MirrorPage";
import { SettingsPage } from "@/pages/SettingsPage";
import { api } from "@/services/api";
import { useDeviceStore } from "@/stores/deviceStore";
import { useMirrorStore } from "@/stores/mirrorStore";
import { useNavigation } from "@/stores/navigationStore";
import type { DevicesPayload } from "@/types";

function useBackendEvents() {
  useEffect(() => {
    const applyDevices = (payload: DevicesPayload) => {
      useDeviceStore.getState().apply(payload);
      useMirrorStore.getState().handleDevices(payload.devices);
    };
    const subscriptions = [
      api.onDevicesChanged(applyDevices),
      api.onMirrorEnded(({ session, reason }) => useMirrorStore.getState().handleEnded(session, reason)),
    ];
    void api.getDevices().then(applyDevices);
    return () => {
      subscriptions.forEach((subscription) => void subscription.then((unlisten) => unlisten()));
    };
  }, []);
}

export default function App() {
  useBackendEvents();
  const page = useNavigation((state) => state.page);

  return (
    <AppShell>
      {page === "dashboard" && <DashboardPage />}
      {page === "devices" && <DevicesPage />}
      {page === "apps" && <AppsPage />}
      {page === "settings" && <SettingsPage />}
      {page === "about" && <AboutPage />}
      {/* Stays mounted so a running session survives moving between pages. */}
      <div className={page === "mirror" ? "h-full" : "hidden"}>
        <MirrorPage />
      </div>
    </AppShell>
  );
}
