import { useEffect, useMemo, useState } from "react";
import { AppWindow, Columns3, LoaderCircle, PictureInPicture2, Search } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Select } from "@/components/ui/select";
import { api, errorMessage } from "@/services/api";
import { deviceName, useDeviceStore } from "@/stores/deviceStore";
import type { AppInfo } from "@/types";

const cachedApps = new Map<string, AppInfo[]>();

export function AppsPage() {
  const allDevices = useDeviceStore((state) => state.devices);
  const devices = useMemo(() => allDevices.filter((device) => device.state === "online"), [allDevices]);
  const [chosen, setChosen] = useState<string | null>(null);
  const [apps, setApps] = useState<AppInfo[] | null>(null);
  const [query, setQuery] = useState("");
  const [error, setError] = useState<string | null>(null);

  const serial = devices.some((device) => device.serial === chosen) ? chosen : (devices[0]?.serial ?? null);
  const device = devices.find((candidate) => candidate.serial === serial);

  useEffect(() => {
    if (!serial) {
      setApps(null);
      return;
    }
    const cached = cachedApps.get(serial);
    if (cached) {
      setApps(cached);
      return;
    }
    let current = true;
    setApps(null);
    setError(null);
    api
      .listApps(serial)
      .then((list) => {
        cachedApps.set(serial, list);
        if (current) setApps(list);
      })
      .catch((cause) => {
        if (current) setError(errorMessage(cause));
      });
    return () => {
      current = false;
    };
  }, [serial]);

  const visible = useMemo(() => {
    const needle = query.trim().toLowerCase();
    return (apps ?? []).filter(
      (app) => app.name.toLowerCase().includes(needle) || app.package.includes(needle),
    );
  }, [apps, query]);

  const open = (appPackage: string | null, title: string) => {
    if (serial) {
      setError(null);
      api.openMirrorWindow(serial, appPackage, title).catch((cause) => setError(errorMessage(cause)));
    }
  };

  return (
    <div className="page-enter flex h-full flex-col gap-5 p-8">
      <header className="flex flex-wrap items-center gap-2">
        <div className="mr-auto">
          <h1 className="font-display text-2xl font-semibold">App Windows</h1>
          <p className="text-[13px] text-muted-foreground">
            Open each app in its own phone window. Press Alt + ← or → to switch between them.
          </p>
        </div>
        {devices.length > 1 && (
          <Select aria-label="Device" value={serial ?? ""} onChange={(event) => setChosen(event.target.value)}>
            {devices.map((candidate) => (
              <option key={candidate.serial} value={candidate.serial}>
                {deviceName(candidate)}
              </option>
            ))}
          </Select>
        )}
        <Button onClick={() => void api.arrangeMirrorWindows()}>
          <Columns3 />
          Arrange side by side
        </Button>
        <Button variant="primary" disabled={!device} onClick={() => device && open(null, deviceName(device))}>
          <PictureInPicture2 />
          Detach whole phone
        </Button>
      </header>

      {error && <p className="text-[13px] text-danger select-text">{error}</p>}

      {!serial ? (
        <div className="flex flex-1 flex-col items-center justify-center gap-3 text-center">
          <AppWindow className="size-9 text-muted-foreground" />
          <p className="font-medium">Connect a device to see its apps</p>
        </div>
      ) : (
        <>
          <label className="relative block max-w-sm">
            <Search className="absolute top-2.5 left-3 size-4 text-muted-foreground" />
            <input
              type="search"
              aria-label="Search apps"
              placeholder="Search apps"
              value={query}
              onChange={(event) => setQuery(event.target.value)}
              className="h-9 w-full rounded-lg border bg-card pr-3 pl-9 text-[13px] select-text"
            />
          </label>
          {apps === null && !error ? (
            <div className="flex items-center gap-2 text-[13px] text-muted-foreground">
              <LoaderCircle className="size-4 animate-spin" />
              Reading apps from the phone…
            </div>
          ) : (
            <div className="grid min-h-0 flex-1 auto-rows-min gap-2 overflow-y-auto [grid-template-columns:repeat(auto-fill,minmax(220px,1fr))]">
              {visible.map((app) => (
                <button
                  key={app.package}
                  title={app.package}
                  data-package={app.package}
                  onClick={() => open(app.package, app.name)}
                  className="truncate rounded-lg border bg-card px-3 py-2.5 text-left text-[13px] font-medium transition-colors hover:bg-muted"
                >
                  {app.name}
                </button>
              ))}
              {apps !== null && visible.length === 0 && (
                <p className="text-[13px] text-muted-foreground">No apps match your search.</p>
              )}
            </div>
          )}
        </>
      )}
    </div>
  );
}
