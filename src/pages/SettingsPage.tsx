import type { ReactNode } from "react";
import { OptionFields } from "@/components/OptionFields";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { useDeviceStore } from "@/stores/deviceStore";
import { useSettingsStore, type Theme } from "@/stores/settingsStore";

function Row({ title, hint, children }: { title: string; hint: string; children: ReactNode }) {
  return (
    <div className="flex items-center justify-between gap-6 px-5 py-4">
      <div className="min-w-0">
        <div className="font-medium">{title}</div>
        <div className="truncate text-[13px] text-muted-foreground select-text">{hint}</div>
      </div>
      <div className="flex shrink-0 gap-2">{children}</div>
    </div>
  );
}

function OnOff({ value, label, onChange }: { value: boolean; label: string; onChange: (value: boolean) => void }) {
  return (
    <Button
      size="sm"
      variant={value ? "primary" : "secondary"}
      role="switch"
      aria-checked={value}
      aria-label={label}
      onClick={() => onChange(!value)}
    >
      {value ? "On" : "Off"}
    </Button>
  );
}

const THEMES: { id: Theme; label: string }[] = [
  { id: "dark", label: "Dark" },
  { id: "light", label: "Light" },
];

export function SettingsPage() {
  const theme = useSettingsStore((state) => state.theme);
  const setTheme = useSettingsStore((state) => state.setTheme);
  const options = useSettingsStore((state) => state.options);
  const setOptions = useSettingsStore((state) => state.setOptions);
  const status = useDeviceStore((state) => state.status);
  const clipboardSync = useSettingsStore((state) => state.clipboardSync);
  const setClipboardSync = useSettingsStore((state) => state.setClipboardSync);

  return (
    <div className="page-enter h-full overflow-y-auto p-8">
      <h1 className="font-display mb-6 text-2xl font-semibold">Settings</h1>
      <Card className="max-w-3xl divide-y">
        <Row title="Theme" hint="How OpenVysor looks">
          {THEMES.map(({ id, label }) => (
            <Button
              key={id}
              size="sm"
              variant={theme === id ? "primary" : "secondary"}
              aria-pressed={theme === id}
              onClick={() => setTheme(id)}
            >
              {label}
            </Button>
          ))}
        </Row>
        <Row title="Mirroring quality" hint="Resolution, bitrate and frame rate for new sessions">
          <OptionFields options={options} onChange={setOptions} />
        </Row>
        <Row title="Phone audio" hint="Plays on this computer. The phone goes silent while mirroring">
          <OnOff
            label="Phone audio"
            value={options.audio}
            onChange={(audio) => setOptions({ ...options, audio })}
          />
        </Row>
        <Row title="Clipboard sync" hint="Text copied on either side, including passwords, reaches the other">
          <OnOff label="Clipboard sync" value={clipboardSync} onChange={setClipboardSync} />
        </Row>
        <Row title="ADB" hint={status.path ?? "Not found"}>
          <span className="text-[13px] text-muted-foreground">
            {status.version ?? "Unavailable"}
          </span>
        </Row>
      </Card>
    </div>
  );
}
