import { create } from "zustand";
import { api, errorMessage } from "@/services/api";
import { MirrorDecoder } from "@/services/videoDecoder";
import { useSettingsStore } from "@/stores/settingsStore";
import type { ControlMessage, Device } from "@/types";

export type MirrorStatus = "idle" | "starting" | "streaming" | "disconnected" | "error";

/** Set in detached windows. The main window uses the serial as its session id. */
export interface SessionTarget {
  session: string;
  package: string | null;
}

interface MirrorStore {
  status: MirrorStatus;
  serial: string | null;
  session: string | null;
  deviceName: string | null;
  message: string | null;
  size: { width: number; height: number };
  fps: number;
  setCanvas: (canvas: HTMLCanvasElement | null) => void;
  setTarget: (target: SessionTarget) => void;
  start: (serial: string) => Promise<void>;
  stop: () => Promise<void>;
  send: (message: ControlMessage) => void;
  handleEnded: (session: string, reason: string) => void;
  handleDevices: (devices: Device[]) => void;
}

let canvasElement: HTMLCanvasElement | null = null;
let target: SessionTarget | null = null;
let decoder: MirrorDecoder | null = null;
// Bumped by every start and stop, so a slow start can tell it has been superseded.
let generation = 0;
// Set once the device vanishes after a drop. Auto-reconnect needs this, or a
// server that keeps crashing on a connected device would restart in a loop.
let deviceWentAway = false;

function closeDecoder() {
  decoder?.close();
  decoder = null;
}

export const useMirrorStore = create<MirrorStore>((set, get) => ({
  status: "idle",
  serial: null,
  session: null,
  deviceName: null,
  message: null,
  size: { width: 0, height: 0 },
  fps: 0,
  setCanvas: (canvas) => {
    canvasElement = canvas;
  },
  setTarget: (next) => {
    target = next;
  },

  start: async (serial) => {
    const current = get();
    if (current.serial && current.serial !== serial && current.status === "streaming") {
      await current.stop();
    }
    if (!canvasElement) {
      set({ status: "error", serial, message: "The video surface is not ready." });
      return;
    }
    if (typeof VideoDecoder === "undefined") {
      set({ status: "error", serial, message: "This WebView2 runtime does not support WebCodecs." });
      return;
    }

    const token = ++generation;
    const session = target?.session ?? serial;
    deviceWentAway = false;
    closeDecoder();
    set({ status: "starting", serial, session, message: null, fps: 0 });

    const active = new MirrorDecoder(canvasElement, {
      onSize: (width, height) => set({ size: { width, height } }),
      onFps: (fps) => set({ fps }),
      onError: (message) => set({ status: "error", message }),
      requestKeyFrame: () => get().send({ type: "resetVideo" }),
    });
    decoder = active;

    try {
      const { options } = useSettingsStore.getState();
      const started = await api.startMirror(session, serial, options, target?.package ?? null, (packet) =>
        active.handlePacket(packet),
      );
      if (token !== generation) {
        await api.stopMirror(session).catch(() => undefined);
        return;
      }
      set({ status: "streaming", deviceName: started.deviceName });
    } catch (error) {
      if (token === generation) {
        closeDecoder();
        set({ status: "error", message: errorMessage(error) });
      }
    }
  },

  stop: async () => {
    const { session } = get();
    generation += 1;
    closeDecoder();
    set({ status: "idle", serial: null, session: null, deviceName: null, message: null, fps: 0 });
    if (session) {
      await api.stopMirror(session).catch(() => undefined);
    }
  },

  send: (message) => {
    const { session, status } = get();
    if (session && status === "streaming") {
      api.sendInput(session, message).catch(() => undefined);
    }
  },

  handleEnded: (session, reason) => {
    if (get().session !== session) {
      return;
    }
    generation += 1;
    closeDecoder();
    deviceWentAway = false;
    set({ status: "disconnected", message: reason, fps: 0 });
  },

  handleDevices: (devices) => {
    const { status, serial, start } = get();
    if (status !== "disconnected" || !serial) {
      return;
    }
    const device = devices.find((candidate) => candidate.serial === serial);
    if (!device || device.state !== "online") {
      deviceWentAway = true;
    } else if (deviceWentAway) {
      void start(serial);
    }
  },
}));
