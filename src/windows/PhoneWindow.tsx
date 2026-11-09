import { useEffect, useRef } from "react";
import type { KeyboardEvent } from "react";
import { LogicalSize } from "@tauri-apps/api/dpi";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { ChevronLeft, Circle, LoaderCircle, Minus, RefreshCw, Unplug, X } from "lucide-react";
import { MirrorSurface } from "@/components/MirrorSurface";
import { Button } from "@/components/ui/button";
import { api } from "@/services/api";
import { useMirrorStore } from "@/stores/mirrorStore";
import type { PhoneWindowInfo } from "@/types";
import {
  ACTION_DOWN,
  ACTION_UP,
  KEYCODE_BACK,
  KEYCODE_HOME,
  PHONE_STRIP_HEIGHT,
  phoneWindowSize,
} from "@/utils/input";

const stripButton = "size-7 rounded-md [&_svg]:size-3.5";

export function PhoneWindow({ info }: { info: PhoneWindowInfo }) {
  const status = useMirrorStore((state) => state.status);
  const message = useMirrorStore((state) => state.message);
  const size = useMirrorStore((state) => state.size);
  const start = useMirrorStore((state) => state.start);
  const send = useMirrorStore((state) => state.send);
  const started = useRef(false);
  const streaming = status === "streaming";

  useEffect(() => {
    const store = useMirrorStore.getState();
    store.setTarget({ session: info.session, package: info.package });
    if (!started.current) {
      started.current = true;
      void store.start(info.serial);
    }
    const subscriptions = [
      api.onMirrorEnded(({ session, reason }) => useMirrorStore.getState().handleEnded(session, reason)),
      api.onDevicesChanged(({ devices }) => useMirrorStore.getState().handleDevices(devices)),
    ];
    return () => {
      subscriptions.forEach((subscription) => void subscription.then((unlisten) => unlisten()));
    };
  }, [info]);

  useEffect(() => {
    const next = phoneWindowSize(size);
    if (next) {
      void getCurrentWindow().setSize(new LogicalSize(next.width, next.height));
    }
  }, [size]);

  const pressKey = (keycode: number) => {
    for (const action of [ACTION_DOWN, ACTION_UP]) {
      send({ type: "keycode", action, keycode, repeat: 0, metastate: 0 });
    }
  };

  // Runs in the capture phase so these keys switch windows instead of reaching the phone.
  const onKeyDownCapture = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.altKey && (event.code === "ArrowLeft" || event.code === "ArrowRight")) {
      event.preventDefault();
      event.stopPropagation();
      void api.focusAdjacentWindow(event.code === "ArrowLeft" ? -1 : 1);
    }
  };

  return (
    <div className="flex h-full flex-col bg-black text-white" onKeyDownCapture={onKeyDownCapture}>
      <header
        data-tauri-drag-region
        style={{ height: PHONE_STRIP_HEIGHT }}
        className="flex shrink-0 items-center gap-0.5 bg-[#101316] pr-1 pl-3"
      >
        <span data-tauri-drag-region className="mr-auto truncate text-xs font-medium">
          {info.title}
        </span>
        <Button
          variant="ghost"
          size="icon"
          className={stripButton}
          title="Back"
          aria-label="Back"
          disabled={!streaming}
          onClick={() => pressKey(KEYCODE_BACK)}
        >
          <ChevronLeft />
        </Button>
        {info.package === null && (
          <Button
            variant="ghost"
            size="icon"
            className={stripButton}
            title="Home"
            aria-label="Home"
            disabled={!streaming}
            onClick={() => pressKey(KEYCODE_HOME)}
          >
            <Circle />
          </Button>
        )}
        <Button
          variant="ghost"
          size="icon"
          className={stripButton}
          title="Minimize"
          aria-label="Minimize"
          onClick={() => void getCurrentWindow().minimize()}
        >
          <Minus />
        </Button>
        <Button
          variant="ghost"
          size="icon"
          className={stripButton}
          title="Close"
          aria-label="Close"
          onClick={() => void getCurrentWindow().close()}
        >
          <X />
        </Button>
      </header>

      <div className="relative min-h-0 flex-1">
        <div className={streaming ? "size-full" : "invisible size-full"}>
          <MirrorSurface />
        </div>
        {!streaming && (
          <div className="absolute inset-0 flex flex-col items-center justify-center gap-3 p-6 text-center">
            {status === "starting" || status === "idle" ? (
              <LoaderCircle className="size-7 animate-spin text-primary" />
            ) : (
              <>
                <Unplug className="size-7 text-warning" />
                <p className="text-[13px] text-neutral-300 select-text">
                  {status === "disconnected"
                    ? "The device disconnected. Plug it back in to resume."
                    : message}
                </p>
                <Button variant="primary" size="sm" onClick={() => void start(info.serial)}>
                  <RefreshCw />
                  Reconnect
                </Button>
              </>
            )}
          </div>
        )}
      </div>
    </div>
  );
}
