import { create } from "zustand";
import type { AdbStatus, ConnectionType, Device, DevicesPayload } from "@/types";
import { readJson, writeJson } from "@/utils/storage";

const RECENTS_KEY = "openvysor.recentDevices";
const MAX_RECENTS = 6;

export interface RecentDevice {
  serial: string;
  name: string;
  connection: ConnectionType;
  lastSeen: number;
}

export function deviceName(device: Device): string {
  return device.details?.model ?? device.model ?? "Android device";
}

export function mergeRecents(recents: RecentDevice[], devices: Device[], now: number): RecentDevice[] {
  const online = devices.filter((device) => device.state === "online");
  const updated: RecentDevice[] = online.map((device) => ({
    serial: device.serial,
    name: deviceName(device),
    connection: device.connection,
    lastSeen: now,
  }));
  const others = recents.filter((recent) => !online.some((d) => d.serial === recent.serial));
  return [...updated, ...others].slice(0, MAX_RECENTS);
}

interface DeviceStore {
  loaded: boolean;
  status: AdbStatus;
  devices: Device[];
  recents: RecentDevice[];
  apply: (payload: DevicesPayload) => void;
}

export const useDeviceStore = create<DeviceStore>((set, get) => ({
  loaded: false,
  status: { available: false, path: null, version: null, error: null },
  devices: [],
  recents: readJson<RecentDevice[]>(RECENTS_KEY, []),
  apply: ({ status, devices }) => {
    const recents = mergeRecents(get().recents, devices, Date.now());
    writeJson(RECENTS_KEY, recents);
    set({ loaded: true, status, devices, recents });
  },
}));
