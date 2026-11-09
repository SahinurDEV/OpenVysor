import { useEffect, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  AudioLines,
  Camera,
  ChevronLeft,
  Circle,
  LoaderCircle,
  Maximize,
  Minimize,
  PictureInPicture2,
  Power,
  RefreshCw,
  RotateCw,
  Smartphone,
  Square,
  Unplug,
  Video,
  Volume1,
  Volume2,
} from "lucide-react";
import { DropOverlay } from "@/components/DropOverlay";
import { MirrorSurface } from "@/components/MirrorSurface";
import { OptionFields } from "@/components/OptionFields";
import { Button } from "@/components/ui/button";
import { api, errorMessage } from "@/services/api";
import { useMirrorStore } from "@/stores/mirrorStore";
import { useNavigation } from "@/stores/navigationStore";
import { useSettingsStore } from "@/stores/settingsStore";
import type { MirrorOptions } from "@/types";
import {
  ACTION_DOWN,
  ACTION_UP,
  KEYCODE_APP_SWITCH,
  KEYCODE_HOME,
  KEYCODE_POWER,
  KEYCODE_VOLUME_DOWN,
  KEYCODE_VOLUME_UP,
} from "@/utils/input";

function Overlay({ children }: { children: React.ReactNode }) {
  return (
    <div className="absolute inset-0 flex flex-col items-center justify-center gap-3 p-6 text-center">
      {children}
    </div>
  );
}

export function MirrorPage() {
  const status = useMirrorStore((state) => state.status);
  const serial = useMirrorStore((state) => state.serial);
  const deviceName = useMirrorStore((state) => state.deviceName);
  const message = useMirrorStore((state) => state.message);
  const fps = useMirrorStore((state) => state.fps);
  const size = useMirrorStore((state) => state.size);
  const start = useMirrorStore((state) => state.start);
  const stop = useMirrorStore((state) => state.stop);
  const send = useMirrorStore((state) => state.send);
  const options = useSettingsStore((state) => state.options);
  const setOptions = useSettingsStore((state) => state.setOptions);
  const navigate = useNavigation((state) => state.navigate);
  const page = useNavigation((state) => state.page);
  const audioPlaying = useMirrorStore((state) => state.audioPlaying);
  const recording = useMirrorStore((state) => state.recording);
  const startRecording = useMirrorStore((state) => state.startRecording);
  const stopRecording = useMirrorStore((state) => state.stopRecording);

  const [fullscreen, setFullscreen] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  const streaming = status === "streaming";

  useEffect(() => {
    if (!notice) {
      return;
    }
    const timer = setTimeout(() => setNotice(null), 5000);
    return () => clearTimeout(timer);
  }, [notice]);

  const pressKey = (keycode: number) => {
    for (const action of [ACTION_DOWN, ACTION_UP]) {
      send({ type: "keycode", action, keycode, repeat: 0, metastate: 0 });
    }
  };

  const pressBack = () => {
    send({ type: "backOrScreenOn", action: ACTION_DOWN });
    send({ type: "backOrScreenOn", action: ACTION_UP });
  };

  const toggleFullscreen = async () => {
    const next = !fullscreen;
    await getCurrentWindow().setFullscreen(next);
    setFullscreen(next);
  };

  const screenshot = async () => {
    if (!serial) {
      return;
    }
    try {
      setNotice(`Saved to ${await api.takeScreenshot(serial)}`);
    } catch (error) {
      setNotice(errorMessage(error));
    }
  };

  const changeOptions = async (next: MirrorOptions) => {
    setOptions(next);
    if (serial && streaming) {
      const target = serial;
      await stop();
      await start(target);
    }
  };

  const toggleRecording = async () => {
    if (!recording) {
      await startRecording();
      return;
    }
    const path = await stopRecording();
    setNotice(path ? `Saved to ${path}` : "The recording could not be saved.");
  };

  const detach = async () => {
    if (!serial) {
      return;
    }
    const target = serial;
    const title = deviceName ?? "Phone";
    if (fullscreen) {
      await toggleFullscreen();
    }
    await stop();
    try {
      await api.openMirrorWindow(target, null, title);
    } catch (error) {
      setNotice(errorMessage(error));
    }
  };

  const disconnect = async () => {
    if (fullscreen) {
      await toggleFullscreen();
    }
    await stop();
  };

  const navButtons = [
    { label: "Back", icon: ChevronLeft, action: pressBack },
    { label: "Home", icon: Circle, action: () => pressKey(KEYCODE_HOME) },
    { label: "Recent apps", icon: Square, action: () => pressKey(KEYCODE_APP_SWITCH) },
    { label: "Volume up", icon: Volume2, action: () => pressKey(KEYCODE_VOLUME_UP) },
    { label: "Volume down", icon: Volume1, action: () => pressKey(KEYCODE_VOLUME_DOWN) },
    { label: "Power", icon: Power, action: () => pressKey(KEYCODE_POWER) },
    { label: "Rotate", icon: RotateCw, action: () => send({ type: "rotateDevice" }) },
  ];

  return (
    <div className="flex h-full flex-col">
      <header className="flex flex-wrap items-center gap-2 border-b px-4 py-2.5">
        <div className="mr-auto min-w-0">
          <h1 className="font-display truncate font-semibold">{deviceName ?? "Screen mirroring"}</h1>
          <p className="text-xs text-muted-foreground" data-testid="mirror-stats">
            {streaming ? `${size.width}×${size.height} · ${fps} FPS` : "Not streaming"}
          </p>
        </div>
        <OptionFields options={options} disabled={status === "starting"} onChange={changeOptions} />
        <Button
          variant="ghost"
          size="icon"
          title="Phone audio"
          aria-label="Phone audio"
          aria-pressed={options.audio}
          data-audio-playing={audioPlaying}
          className={options.audio ? "text-primary" : undefined}
          disabled={status === "starting"}
          onClick={() => void changeOptions({ ...options, audio: !options.audio })}
        >
          <AudioLines />
        </Button>
        <Button
          variant="ghost"
          size="icon"
          title={recording ? "Stop recording" : "Record screen"}
          aria-label={recording ? "Stop recording" : "Record screen"}
          aria-pressed={recording}
          className={recording ? "text-danger" : undefined}
          disabled={!streaming}
          onClick={() => void toggleRecording()}
        >
          <Video />
        </Button>
        <Button variant="ghost" size="icon" title="Screenshot" aria-label="Screenshot" disabled={!streaming} onClick={screenshot}>
          <Camera />
        </Button>
        <Button
          variant="ghost"
          size="icon"
          title="Full screen"
          aria-label="Full screen"
          aria-pressed={fullscreen}
          onClick={toggleFullscreen}
        >
          {fullscreen ? <Minimize /> : <Maximize />}
        </Button>
        <Button variant="ghost" size="icon" title="Detach to a phone window" aria-label="Detach to a phone window" disabled={!streaming} onClick={detach}>
          <PictureInPicture2 />
        </Button>
        <Button variant="danger" size="sm" disabled={!serial} onClick={disconnect}>
          <Unplug />
          Disconnect
        </Button>
      </header>

      <div className="flex min-h-0 flex-1">
        <div className="relative min-w-0 flex-1 bg-muted/40 p-4">
          <div className={streaming ? "size-full" : "invisible size-full"}>
            <MirrorSurface />
          </div>

          {status === "idle" && (
            <Overlay>
              <Smartphone className="size-9 text-muted-foreground" />
              <p className="font-medium">No device is being mirrored</p>
              <Button variant="primary" size="sm" onClick={() => navigate("devices")}>
                Choose a device
              </Button>
            </Overlay>
          )}
          {status === "starting" && (
            <Overlay>
              <LoaderCircle className="size-8 animate-spin text-primary" />
              <p className="text-muted-foreground">Starting the stream…</p>
            </Overlay>
          )}
          {(status === "disconnected" || status === "error") && (
            <Overlay>
              <Unplug className="size-9 text-warning" />
              <p className="font-medium">
                {status === "disconnected" ? "The device disconnected" : "Mirroring could not start"}
              </p>
              <p className="max-w-md text-[13px] text-muted-foreground select-text">
                {status === "disconnected"
                  ? "Plug it back in and mirroring resumes automatically."
                  : message}
              </p>
              {serial && (
                <Button variant="primary" size="sm" onClick={() => void start(serial)}>
                  <RefreshCw />
                  Reconnect
                </Button>
              )}
            </Overlay>
          )}
          <DropOverlay serial={serial} enabled={streaming && page === "mirror"} />
          {notice && (
            <div className="absolute bottom-4 left-1/2 max-w-[80%] -translate-x-1/2 truncate rounded-lg border bg-card px-3 py-2 text-[13px] shadow-lg select-text">
              {notice}
            </div>
          )}
        </div>

        <nav aria-label="Device controls" className="flex w-13 flex-col items-center gap-1 border-l py-3">
          {navButtons.map(({ label, icon: Icon, action }) => (
            <Button
              key={label}
              variant="ghost"
              size="icon"
              title={label}
              aria-label={label}
              disabled={!streaming}
              onClick={action}
            >
              <Icon />
            </Button>
          ))}
        </nav>
      </div>
    </div>
  );
}
