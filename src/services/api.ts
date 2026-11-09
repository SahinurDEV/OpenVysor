import { Channel, invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  AppErrorPayload,
  AppInfo,
  ControlMessage,
  DevicesPayload,
  MirrorEnded,
  MirrorOptions,
} from "@/types";

export function errorMessage(error: unknown): string {
  if (typeof error === "object" && error !== null && "message" in error) {
    return (error as AppErrorPayload).message;
  }
  return String(error);
}

export const api = {
  getDevices: () => invoke<DevicesPayload>("get_devices"),

  startMirror: (
    session: string,
    serial: string,
    options: MirrorOptions,
    appPackage: string | null,
    onPacket: (packet: ArrayBuffer) => void,
  ) => {
    const channel = new Channel<ArrayBuffer>();
    channel.onmessage = onPacket;
    return invoke<{ deviceName: string }>("start_mirror", {
      session,
      serial,
      options,
      package: appPackage,
      channel,
    });
  },

  stopMirror: (session: string) => invoke<void>("stop_mirror", { session }),

  sendInput: (session: string, message: ControlMessage) =>
    invoke<void>("mirror_input", { session, message }),

  listApps: (serial: string) => invoke<AppInfo[]>("list_apps", { serial }),

  openMirrorWindow: (serial: string, appPackage: string | null, title: string) =>
    invoke<string>("open_mirror_window", { serial, package: appPackage, title }),

  arrangeMirrorWindows: () => invoke<void>("arrange_mirror_windows"),

  focusAdjacentWindow: (direction: number) => invoke<void>("focus_adjacent_window", { direction }),

  takeScreenshot: (serial: string) => invoke<string>("take_screenshot", { serial }),

  onDevicesChanged: (handler: (payload: DevicesPayload) => void): Promise<UnlistenFn> =>
    listen<DevicesPayload>("devices-changed", (event) => handler(event.payload)),

  onMirrorEnded: (handler: (payload: MirrorEnded) => void): Promise<UnlistenFn> =>
    listen<MirrorEnded>("mirror-ended", (event) => handler(event.payload)),
};
