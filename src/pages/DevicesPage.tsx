import { Usb } from "lucide-react";
import { AdbGuide } from "@/components/AdbGuide";
import { DeviceCard } from "@/components/DeviceCard";
import { useDeviceStore } from "@/stores/deviceStore";

export function DevicesPage() {
  const loaded = useDeviceStore((state) => state.loaded);
  const status = useDeviceStore((state) => state.status);
  const devices = useDeviceStore((state) => state.devices);

  return (
    <div className="page-enter h-full overflow-y-auto p-8">
      <h1 className="font-display mb-6 text-2xl font-semibold">Devices</h1>
      {loaded && !status.available && <AdbGuide />}
      {status.available && devices.length === 0 && (
        <div className="flex flex-col items-center gap-3 rounded-xl border border-dashed py-16 text-center">
          <Usb className="size-9 text-muted-foreground" />
          <p className="font-medium">No devices connected</p>
          <ol className="list-decimal space-y-1 pl-5 text-left text-[13px] text-muted-foreground">
            <li>On your phone, open Settings, then Developer options</li>
            <li>Turn on USB debugging</li>
            <li>Connect the phone with a USB data cable and tap Allow</li>
          </ol>
        </div>
      )}
      <div className="grid gap-4 [grid-template-columns:repeat(auto-fill,minmax(320px,1fr))]">
        {devices.map((device) => (
          <DeviceCard key={device.serial} device={device} />
        ))}
      </div>
    </div>
  );
}
