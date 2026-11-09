import type { ComponentType } from "react";
import { Cable, MonitorSmartphone, Smartphone, Wifi } from "lucide-react";
import { AdbGuide } from "@/components/AdbGuide";
import { Logo } from "@/components/Logo";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { useDeviceStore } from "@/stores/deviceStore";
import { useMirrorStore } from "@/stores/mirrorStore";
import { useNavigation } from "@/stores/navigationStore";

function Stat({
  icon: Icon,
  label,
  value,
}: {
  icon: ComponentType<{ className?: string }>;
  label: string;
  value: number;
}) {
  return (
    <Card className="flex items-center gap-4 p-5">
      <Icon className="size-5 text-muted-foreground" />
      <div>
        <div className="font-display text-2xl leading-none font-semibold tabular-nums">{value}</div>
        <div className="mt-1 text-[13px] text-muted-foreground">{label}</div>
      </div>
    </Card>
  );
}

const timeFormat = new Intl.DateTimeFormat(undefined, { dateStyle: "medium", timeStyle: "short" });

export function DashboardPage() {
  const loaded = useDeviceStore((state) => state.loaded);
  const status = useDeviceStore((state) => state.status);
  const devices = useDeviceStore((state) => state.devices);
  const recents = useDeviceStore((state) => state.recents);
  const start = useMirrorStore((state) => state.start);
  const navigate = useNavigation((state) => state.navigate);

  const online = devices.filter((device) => device.state === "online");
  const usb = online.filter((device) => device.connection === "usb").length;
  const wireless = online.filter((device) => device.connection === "wireless").length;
  const needsAttention = devices.length - online.length;
  const first = online[0];

  const mirrorFirst = () => {
    navigate("mirror");
    void start(first.serial);
  };

  return (
    <div className="page-enter h-full space-y-6 overflow-y-auto p-8">
      <header className="flex items-center gap-4">
        <Logo size={44} />
        <div className="mr-auto">
          <h1 className="font-display text-2xl leading-tight font-semibold">OpenVysor</h1>
          <p className="text-[13px] text-muted-foreground" data-testid="connection-summary">
            {online.length === 0
              ? "Waiting for a device"
              : `${online.length} device${online.length === 1 ? "" : "s"} ready`}
            {needsAttention > 0 && ` · ${needsAttention} need attention`}
          </p>
        </div>
        <Button variant="secondary" onClick={() => navigate("devices")}>
          All devices
        </Button>
        <Button variant="primary" disabled={!first} onClick={mirrorFirst}>
          <MonitorSmartphone />
          Mirror {first ? (first.details?.model ?? first.model ?? "device") : "device"}
        </Button>
      </header>

      {loaded && !status.available && <AdbGuide />}

      <section className="grid grid-cols-3 gap-4">
        <Stat icon={Smartphone} label="Connected" value={online.length} />
        <Stat icon={Cable} label="USB" value={usb} />
        <Stat icon={Wifi} label="Wireless" value={wireless} />
      </section>

      <section>
        <h2 className="mb-3 font-semibold">Recent devices</h2>
        {recents.length === 0 ? (
          <p className="text-[13px] text-muted-foreground">
            Devices you connect appear here so you can see when each was last available.
          </p>
        ) : (
          <Card className="divide-y">
            {recents.map((recent) => {
              const connected = online.some((device) => device.serial === recent.serial);
              return (
                <div key={recent.serial} className="flex items-center gap-3 px-5 py-3">
                  <span className={`size-2 rounded-full ${connected ? "bg-success" : "bg-border"}`} />
                  <span className="mr-auto font-medium">{recent.name}</span>
                  <span className="text-[13px] text-muted-foreground">
                    {connected ? "Connected now" : timeFormat.format(recent.lastSeen)}
                  </span>
                </div>
              );
            })}
          </Card>
        )}
      </section>
    </div>
  );
}
