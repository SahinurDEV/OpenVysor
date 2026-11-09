import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Device } from "@/types";

const { startMirror, stopMirror, sendInput } = vi.hoisted(() => ({
  startMirror: vi.fn(),
  stopMirror: vi.fn(),
  sendInput: vi.fn(() => Promise.resolve()),
}));

vi.mock("@/services/api", () => ({
  api: { startMirror, stopMirror, sendInput },
  errorMessage: (error: { message?: string }) => error.message ?? String(error),
}));

vi.mock("@/services/videoDecoder", () => ({
  MirrorDecoder: class {
    handlePacket() {}
    close() {}
  },
}));

import { deviceName, mergeRecents } from "@/stores/deviceStore";
import { useMirrorStore } from "@/stores/mirrorStore";

function device(serial: string, state: Device["state"] = "online"): Device {
  return { serial, state, connection: "usb", model: "A059", details: null };
}

describe("deviceName", () => {
  it("falls back when the model is unknown", () => {
    expect(deviceName(device("a"))).toBe("A059");
    expect(deviceName({ ...device("a"), model: null })).toBe("Android device");
  });
});

describe("mergeRecents", () => {
  it("records online devices only and keeps older entries", () => {
    const old = { serial: "old", name: "Old", connection: "usb" as const, lastSeen: 1 };
    const merged = mergeRecents([old], [device("new"), device("locked", "unauthorized")], 5);
    expect(merged.map((entry) => entry.serial)).toEqual(["new", "old"]);
    expect(merged[0].lastSeen).toBe(5);
  });

  it("does not duplicate a device that reconnects", () => {
    const seen = { serial: "a", name: "A059", connection: "usb" as const, lastSeen: 1 };
    expect(mergeRecents([seen], [device("a")], 9)).toHaveLength(1);
  });
});

describe("mirror store", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.stubGlobal("VideoDecoder", class {});
    startMirror.mockResolvedValue({ deviceName: "A059" });
    stopMirror.mockResolvedValue(undefined);
    useMirrorStore.setState({ status: "idle", serial: null, session: null, deviceName: null, message: null });
    useMirrorStore.getState().setTarget(null as never);
    useMirrorStore.getState().setCanvas(document.createElement("canvas"));
  });

  it("moves from starting to streaming", async () => {
    await useMirrorStore.getState().start("a");
    expect(useMirrorStore.getState().status).toBe("streaming");
    expect(useMirrorStore.getState().deviceName).toBe("A059");
  });

  it("reports a failed start", async () => {
    startMirror.mockRejectedValue({ code: "server_start", message: "boom" });
    await useMirrorStore.getState().start("a");
    expect(useMirrorStore.getState().status).toBe("error");
    expect(useMirrorStore.getState().message).toBe("boom");
  });

  it("stops a session whose start finished after the user cancelled", async () => {
    let finish = (_value: unknown) => {};
    startMirror.mockReturnValue(new Promise((resolve) => (finish = resolve)));
    const pending = useMirrorStore.getState().start("a");
    await useMirrorStore.getState().stop();
    finish({ deviceName: "A059" });
    await pending;
    expect(useMirrorStore.getState().status).toBe("idle");
    expect(stopMirror).toHaveBeenCalledWith("a");
  });

  it("reconnects only after the device has gone away and returned", async () => {
    await useMirrorStore.getState().start("a");
    startMirror.mockClear();
    const store = useMirrorStore.getState();

    store.handleEnded("a", "connection closed");
    expect(useMirrorStore.getState().status).toBe("disconnected");

    store.handleDevices([device("a")]);
    expect(startMirror).not.toHaveBeenCalled();

    store.handleDevices([]);
    store.handleDevices([device("a")]);
    expect(startMirror).toHaveBeenCalledTimes(1);
  });

  it("uses the window's session and app in a detached window", async () => {
    useMirrorStore.getState().setTarget({ session: "mirror-1", package: "com.a.b" });
    await useMirrorStore.getState().start("a");
    expect(startMirror.mock.calls[0].slice(0, 2)).toEqual(["mirror-1", "a"]);
    expect(startMirror.mock.calls[0][3]).toBe("com.a.b");

    useMirrorStore.getState().send({ type: "resetVideo" });
    expect(sendInput).toHaveBeenCalledWith("mirror-1", { type: "resetVideo" });

    useMirrorStore.getState().handleEnded("a", "closed");
    expect(useMirrorStore.getState().status).toBe("streaming");
    useMirrorStore.getState().handleEnded("mirror-1", "closed");
    expect(useMirrorStore.getState().status).toBe("disconnected");
  });

  it("ignores an end event from another device", async () => {
    await useMirrorStore.getState().start("a");
    useMirrorStore.getState().handleEnded("b", "closed");
    expect(useMirrorStore.getState().status).toBe("streaming");
  });
});
