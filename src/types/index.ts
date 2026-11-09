export type DeviceState =
  | "online"
  | "offline"
  | "unauthorized"
  | "authorizing"
  | "connecting"
  | "noPermissions"
  | "other";

export type ConnectionType = "usb" | "wireless" | "emulator";

export interface DeviceDetails {
  manufacturer: string | null;
  model: string | null;
  androidVersion: string | null;
  sdk: string | null;
  resolution: string | null;
  battery: number | null;
}

export interface Device {
  serial: string;
  state: DeviceState;
  connection: ConnectionType;
  model: string | null;
  details: DeviceDetails | null;
}

export interface AdbStatus {
  available: boolean;
  path: string | null;
  version: string | null;
  error: string | null;
}

export interface DevicesPayload {
  status: AdbStatus;
  devices: Device[];
}

export interface MirrorOptions {
  maxSize: number;
  bitRate: number;
  maxFps: number;
}

export interface MirrorEnded {
  session: string;
  reason: string;
}

export interface AppErrorPayload {
  code: string;
  message: string;
}

export type ControlMessage =
  | { type: "keycode"; action: number; keycode: number; repeat: number; metastate: number }
  | { type: "text"; text: string }
  | {
      type: "touch";
      action: number;
      pointerId: number;
      x: number;
      y: number;
      width: number;
      height: number;
      pressure: number;
      actionButton: number;
      buttons: number;
    }
  | {
      type: "scroll";
      x: number;
      y: number;
      width: number;
      height: number;
      hscroll: number;
      vscroll: number;
      buttons: number;
    }
  | { type: "backOrScreenOn"; action: number }
  | { type: "rotateDevice" }
  | { type: "resetVideo" };

export interface AppInfo {
  name: string;
  package: string;
  system: boolean;
}

/** Injected by the backend into detached phone windows. */
export interface PhoneWindowInfo {
  session: string;
  serial: string;
  package: string | null;
  title: string;
}

declare global {
  interface Window {
    __OPENVYSOR_PHONE__?: PhoneWindowInfo;
  }
}

export type PageId = "dashboard" | "devices" | "mirror" | "apps" | "settings" | "about";
