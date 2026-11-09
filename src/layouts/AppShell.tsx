import type { ReactNode } from "react";
import {
  AppWindow,
  FolderOpen,
  Info,
  LayoutDashboard,
  MonitorSmartphone,
  Package,
  Settings,
  Smartphone,
} from "lucide-react";
import { Logo } from "@/components/Logo";
import { useDeviceStore } from "@/stores/deviceStore";
import { useMirrorStore } from "@/stores/mirrorStore";
import { useNavigation } from "@/stores/navigationStore";
import type { PageId } from "@/types";
import { cn } from "@/utils/cn";

const ITEMS: { id: PageId; label: string; icon: typeof Info }[] = [
  { id: "dashboard", label: "Dashboard", icon: LayoutDashboard },
  { id: "devices", label: "Devices", icon: Smartphone },
  { id: "mirror", label: "Screen Mirroring", icon: MonitorSmartphone },
  { id: "apps", label: "App Windows", icon: AppWindow },
];

const PLANNED = [
  { label: "File Manager", icon: FolderOpen },
  { label: "APK Installer", icon: Package },
];

const FOOTER_ITEMS: { id: PageId; label: string; icon: typeof Info }[] = [
  { id: "settings", label: "Settings", icon: Settings },
  { id: "about", label: "About", icon: Info },
];

const itemClass =
  "flex h-9 w-full items-center gap-3 rounded-lg px-3 text-[13px] font-medium transition-colors [&_svg]:size-4";

function NavItem({ id, label, icon: Icon }: { id: PageId; label: string; icon: typeof Info }) {
  const page = useNavigation((state) => state.page);
  const navigate = useNavigation((state) => state.navigate);
  const streaming = useMirrorStore((state) => state.status === "streaming");
  const active = page === id;
  return (
    <button
      aria-current={active ? "page" : undefined}
      className={cn(
        itemClass,
        active ? "bg-muted text-foreground" : "text-muted-foreground hover:bg-muted/60 hover:text-foreground",
      )}
      onClick={() => navigate(id)}
    >
      <Icon />
      {label}
      {id === "mirror" && streaming && <span className="ml-auto size-2 rounded-full bg-success" />}
    </button>
  );
}

export function AppShell({ children }: { children: ReactNode }) {
  const onlineCount = useDeviceStore(
    (state) => state.devices.filter((device) => device.state === "online").length,
  );

  return (
    <div className="flex h-full">
      <aside className="flex w-56 shrink-0 flex-col border-r p-3">
        <div className="mb-4 flex items-center gap-2.5 px-2 py-1.5">
          <Logo />
          <span className="font-display text-[15px] font-semibold">OpenVysor</span>
        </div>
        <nav className="flex flex-1 flex-col gap-0.5" aria-label="Main">
          {ITEMS.map((item) => (
            <NavItem key={item.id} {...item} />
          ))}
          {PLANNED.map(({ label, icon: Icon }) => (
            <div key={label} aria-disabled="true" className={cn(itemClass, "text-muted-foreground/50")}>
              <Icon />
              {label}
              <span className="ml-auto text-[11px] font-normal">Soon</span>
            </div>
          ))}
          <div className="mt-auto flex flex-col gap-0.5">
            {FOOTER_ITEMS.map((item) => (
              <NavItem key={item.id} {...item} />
            ))}
          </div>
        </nav>
        <div className="mt-3 flex items-center gap-2 border-t px-2 pt-3 text-xs text-muted-foreground">
          <span className={cn("size-2 rounded-full", onlineCount ? "bg-success" : "bg-border")} />
          {onlineCount ? `${onlineCount} connected` : "No device"}
        </div>
      </aside>
      <main className="min-w-0 flex-1">{children}</main>
    </div>
  );
}
