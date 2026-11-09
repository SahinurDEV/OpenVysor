export const ACTION_DOWN = 0;
export const ACTION_UP = 1;
export const ACTION_MOVE = 2;
export const ACTION_CANCEL = 3;
export const POINTER_ID_FINGER = -2;

export const KEYCODE_HOME = 3;
export const KEYCODE_BACK = 4;
export const KEYCODE_VOLUME_UP = 24;
export const KEYCODE_VOLUME_DOWN = 25;
export const KEYCODE_POWER = 26;
export const KEYCODE_APP_SWITCH = 187;

const NAMED_KEYCODES: Record<string, number> = {
  Enter: 66,
  NumpadEnter: 160,
  Backspace: 67,
  Delete: 112,
  Tab: 61,
  Space: 62,
  Escape: 111,
  ArrowUp: 19,
  ArrowDown: 20,
  ArrowLeft: 21,
  ArrowRight: 22,
  Home: 122,
  End: 123,
  PageUp: 92,
  PageDown: 93,
  Insert: 124,
  Minus: 69,
  Equal: 70,
  BracketLeft: 71,
  BracketRight: 72,
  Backslash: 73,
  Semicolon: 74,
  Quote: 75,
  Backquote: 68,
  Comma: 55,
  Period: 56,
  Slash: 76,
  ShiftLeft: 59,
  ShiftRight: 60,
  ControlLeft: 113,
  ControlRight: 114,
  AltLeft: 57,
  AltRight: 58,
  MetaLeft: 117,
  MetaRight: 118,
  CapsLock: 115,
};

/** Maps `KeyboardEvent.code` to an Android keycode. */
export function androidKeycode(code: string): number | null {
  const letter = /^Key([A-Z])$/.exec(code);
  if (letter) {
    return 29 + letter[1].charCodeAt(0) - 65;
  }
  const digit = /^Digit([0-9])$/.exec(code);
  if (digit) {
    return 7 + Number(digit[1]);
  }
  const fn = /^F([1-9]|1[0-2])$/.exec(code);
  if (fn) {
    return 130 + Number(fn[1]);
  }
  return NAMED_KEYCODES[code] ?? null;
}

interface Modifiers {
  shiftKey: boolean;
  altKey: boolean;
  ctrlKey: boolean;
  metaKey: boolean;
  capsLock: boolean;
}

export function androidMetaState(modifiers: Modifiers): number {
  let state = 0;
  if (modifiers.shiftKey) state |= 0x1 | 0x40;
  if (modifiers.altKey) state |= 0x2 | 0x10;
  if (modifiers.ctrlKey) state |= 0x1000 | 0x2000;
  if (modifiers.metaKey) state |= 0x10000 | 0x20000;
  if (modifiers.capsLock) state |= 0x100000;
  return state;
}

/** Text with no physical Android key, such as accented or non-Latin characters. */
export function needsTextInjection(key: string, ctrlKey: boolean, altKey: boolean): boolean {
  if (ctrlKey !== altKey) {
    return false;
  }
  const characters = [...key];
  return characters.length === 1 && key.codePointAt(0)! > 0x7e;
}

export interface Size {
  width: number;
  height: number;
}

export function fitInside(container: Size, content: Size): Size {
  if (!container.width || !container.height || !content.width || !content.height) {
    return { width: 0, height: 0 };
  }
  const scale = Math.min(container.width / content.width, container.height / content.height);
  return { width: Math.floor(content.width * scale), height: Math.floor(content.height * scale) };
}

export const PHONE_STRIP_HEIGHT = 32;
const PHONE_LONG_SIDE = 760;

/** Window size that matches the video, so the phone shape follows rotation. */
export function phoneWindowSize(video: Size): Size | null {
  if (!video.width || !video.height) {
    return null;
  }
  if (video.height >= video.width) {
    return {
      width: Math.round((PHONE_LONG_SIDE * video.width) / video.height),
      height: PHONE_LONG_SIDE + PHONE_STRIP_HEIGHT,
    };
  }
  return {
    width: PHONE_LONG_SIDE,
    height: Math.round((PHONE_LONG_SIDE * video.height) / video.width) + PHONE_STRIP_HEIGHT,
  };
}

interface Rect {
  left: number;
  top: number;
  width: number;
  height: number;
}

/** Converts a client position into video coordinates, clamped to the frame. */
export function toVideoPoint(clientX: number, clientY: number, rect: Rect, video: Size) {
  const clamp = (value: number, max: number) => Math.min(Math.max(value, 0), max);
  return {
    x: clamp(Math.round(((clientX - rect.left) / rect.width) * video.width), video.width - 1),
    y: clamp(Math.round(((clientY - rect.top) / rect.height) * video.height), video.height - 1),
  };
}

/** Browser deltas grow downward and rightward, Android scroll values the opposite way. */
export function wheelToScroll(deltaX: number, deltaY: number) {
  const convert = (delta: number) => Math.min(Math.max(-delta / 100, -16), 16);
  return { hscroll: convert(deltaX), vscroll: convert(deltaY) };
}
