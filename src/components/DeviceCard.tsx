import { useState } from "react";
import { BatteryMedium, Cable, Info, MonitorSmartphone, Unplug, Wifi } from "lucide-react";
import { Button } from "@/components/ui/button";
import { api } from "@/services/api";
import { Card } from "@/components/ui/card";
import { deviceName } from "@/stores/deviceStore";
import { useMirrorStore } from "@/stores/mirrorStore";
import { useNavigation } from "@/stores/navigationStore";
import type { Device, DeviceState } from "@/types";
import { cn } from "@/utils/cn";

const STATE_LABELS: Record<DeviceState, string> = {
  online: "Ready",
  offline: "Offline",
  unauthorized: "Needs authorization",
  authorizing: "Authorizing",
  connecting: "Connecting",
  noPermissions: "No permission",
  other: "Unavailable",
};

const STATE_HELP: Partial<Record<DeviceState, string>> = {
  unauthorized:
    "Unlock your phone and tap Allow on the “Allow USB debugging?” prompt. If no prompt appears, replug the cable.",
  offline: "The phone is not responding. Replug the cable or toggle USB debugging.",
  noPermissions: "Windows could not open this device. Reinstall its USB driver.",
};

function InfoRow({ label, value }: { label: string; value: string | null | undefined }) {
  return (
    <div className="flex justify-between gap-4">
      <dt className="text-muted-foreground">{label}</dt>
      <dd className="truncate select-text">{value ?? "Unknown"}</dd>
    </div>
  );
}

export function DeviceCard({ device }: { device: Device }) {
  const [showInfo, setShowInfo] = useState(false);
  const mirrorSerial = useMirrorStore((state) => state.serial);
  const mirrorStatus = useMirrorStore((state) => state.status);
  const start = useMirrorStore((state) => state.start);
  const stop = useMirrorStore((state) => state.stop);
  const navigate = useNavigation((state) => state.navigate);

  const online = device.state === "online";
  const mirroring = mirrorSerial === device.serial && mirrorStatus === "streaming";
  const details = device.details;
  const ConnectionIcon = device.connection === "wireless" ? Wifi : Cable;
  const connectionLabel = { usb: "USB", wireless: "Wireless", emulator: "Emulator" }[device.connection];
  const help = STATE_HELP[device.state];

  const openMirror = () => {
    navigate("mirror");
    if (!mirroring) {
      void start(device.serial);
    }
  };

  return (
    <Card className="flex flex-col gap-4 p-5">
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0">
          <h3 className="font-display truncate text-[15px] font-semibold">{deviceName(device)}</h3>
          <p className="truncate text-[13px] text-muted-foreground">
            {[details?.manufacturer, details?.androidVersion && `Android ${details.androidVersion}`]
              .filter(Boolean)
              .join(" · ") || "Details unavailable"}
          </p>
        </div>
        <span
          className={cn(
            "flex shrink-0 items-center gap-1.5 rounded-full px-2.5 py-1 text-xs font-medium",
            online ? "bg-success/15 text-success" : "bg-warning/15 text-warning",
          )}
        >
          <span className={cn("size-1.5 rounded-full", online ? "bg-success" : "bg-warning")} />
          {mirroring ? "Mirroring" : STATE_LABELS[device.state]}
        </span>
      </div>

      <div className="flex flex-wrap gap-x-4 gap-y-1 text-[13px] text-muted-foreground">
        <span className="flex items-center gap-1.5">
          <ConnectionIcon className="size-3.5" />
          {connectionLabel}
        </span>
        {details?.resolution && (
          <span className="flex items-center gap-1.5">
            <MonitorSmartphone className="size-3.5" />
            {details.resolution}
          </span>
        )}
        {details?.battery != null && (
          <span className="flex items-center gap-1.5">
            <BatteryMedium className="size-3.5" />
            {details.battery}%
          </span>
        )}
      </div>

      {help && <p className="text-[13px] text-warning">{help}</p>}

      {showInfo && (
        <dl className="space-y-1.5 rounded-lg bg-muted p-3 text-[13px]">
          <InfoRow label="Manufacturer" value={details?.manufacturer} />
          <InfoRow label="Model" value={details?.model ?? device.model} />
          <InfoRow label="Android" value={details?.androidVersion} />
          <InfoRow label="API level" value={details?.sdk} />
          <InfoRow label="Serial" value={device.serial} />
        </dl>
      )}

      <div className="mt-auto flex gap-2">
        <Button variant="primary" size="sm" disabled={!online} onClick={openMirror}>
          <MonitorSmartphone />
          {mirroring ? "Open mirror" : "Mirror screen"}
        </Button>
        {mirroring && (
          <Button variant="danger" size="sm" onClick={() => void stop()}>
            <Unplug />
            Disconnect
          </Button>
        )}
        {device.connection === "wireless" && !mirroring && (
          <Button variant="ghost" size="sm" onClick={() => void api.wirelessDisconnect(device.serial)}>
            <Unplug />
            Disconnect
          </Button>
        )}
        <Button
          variant="ghost"
          size="sm"
          aria-expanded={showInfo}
          onClick={() => setShowInfo((value) => !value)}
        >
          <Info />
          Info
        </Button>
      </div>
    </Card>
  );
}
