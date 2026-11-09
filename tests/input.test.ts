import { describe, expect, it } from "vitest";
import { h264CodecString } from "@/utils/h264";
import {
  androidKeycode,
  androidMetaState,
  fitInside,
  needsTextInjection,
  phoneWindowSize,
  toVideoPoint,
  wheelToScroll,
} from "@/utils/input";

describe("androidKeycode", () => {
  it("maps letters, digits and function keys", () => {
    expect(androidKeycode("KeyA")).toBe(29);
    expect(androidKeycode("KeyZ")).toBe(54);
    expect(androidKeycode("Digit0")).toBe(7);
    expect(androidKeycode("Digit9")).toBe(16);
    expect(androidKeycode("F1")).toBe(131);
    expect(androidKeycode("F12")).toBe(142);
  });

  it("maps named keys and rejects unknown ones", () => {
    expect(androidKeycode("Enter")).toBe(66);
    expect(androidKeycode("Backspace")).toBe(67);
    expect(androidKeycode("MediaPlayPause")).toBeNull();
  });
});

describe("androidMetaState", () => {
  const none = { shiftKey: false, altKey: false, ctrlKey: false, metaKey: false, capsLock: false };

  it("is zero without modifiers and combines flags", () => {
    expect(androidMetaState(none)).toBe(0);
    expect(androidMetaState({ ...none, shiftKey: true })).toBe(0x41);
    expect(androidMetaState({ ...none, ctrlKey: true })).toBe(0x3000);
  });
});

describe("needsTextInjection", () => {
  it("only selects characters without an Android key", () => {
    expect(needsTextInjection("a", false, false)).toBe(false);
    expect(needsTextInjection("Enter", false, false)).toBe(false);
    expect(needsTextInjection("é", false, false)).toBe(true);
    expect(needsTextInjection("😀", false, false)).toBe(true);
  });

  it("treats AltGr (ctrl and alt together) as typed text", () => {
    expect(needsTextInjection("€", true, true)).toBe(true);
    expect(needsTextInjection("é", true, false)).toBe(false);
  });
});

describe("geometry", () => {
  it("fits a portrait frame by height", () => {
    expect(fitInside({ width: 1000, height: 800 }, { width: 720, height: 1600 })).toEqual({
      width: 360,
      height: 800,
    });
  });

  it("has no size before the video size is known", () => {
    expect(fitInside({ width: 1000, height: 800 }, { width: 0, height: 0 })).toEqual({
      width: 0,
      height: 0,
    });
  });

  it("scales client positions and clamps them to the frame", () => {
    const rect = { left: 100, top: 50, width: 360, height: 800 };
    const video = { width: 720, height: 1600 };
    expect(toVideoPoint(280, 450, rect, video)).toEqual({ x: 360, y: 800 });
    expect(toVideoPoint(0, 0, rect, video)).toEqual({ x: 0, y: 0 });
    expect(toVideoPoint(9999, 9999, rect, video)).toEqual({ x: 719, y: 1599 });
  });

  it("shapes the window to the video and follows rotation", () => {
    expect(phoneWindowSize({ width: 720, height: 1600 })).toEqual({ width: 342, height: 792 });
    expect(phoneWindowSize({ width: 1600, height: 720 })).toEqual({ width: 760, height: 374 });
    expect(phoneWindowSize({ width: 0, height: 0 })).toBeNull();
  });

  it("inverts wheel direction", () => {
    expect(wheelToScroll(0, 100)).toEqual({ hscroll: -0, vscroll: -1 });
    expect(wheelToScroll(0, -99999).vscroll).toBe(16);
  });
});

describe("h264CodecString", () => {
  it("reads the profile from the SPS", () => {
    const config = new Uint8Array([0, 0, 0, 1, 0x67, 0x42, 0xc0, 0x29, 0xaa, 0, 0, 0, 1, 0x68, 0xce]);
    expect(h264CodecString(config)).toBe("avc1.42c029");
  });

  it("returns null without an SPS", () => {
    expect(h264CodecString(new Uint8Array([0, 0, 0, 1, 0x68, 1, 2, 3, 4]))).toBeNull();
  });
});
