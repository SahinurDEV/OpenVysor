import { useEffect, useLayoutEffect, useRef, useState } from "react";
import type { KeyboardEvent, PointerEvent, WheelEvent } from "react";
import { useMirrorStore } from "@/stores/mirrorStore";
import {
  ACTION_CANCEL,
  ACTION_DOWN,
  ACTION_MOVE,
  ACTION_UP,
  KEYCODE_HOME,
  POINTER_ID_FINGER,
  androidKeycode,
  androidMetaState,
  fitInside,
  needsTextInjection,
  toVideoPoint,
  wheelToScroll,
} from "@/utils/input";

const BUTTON_LEFT = 0;
const BUTTON_MIDDLE = 1;
const BUTTON_RIGHT = 2;

export function MirrorSurface() {
  const containerRef = useRef<HTMLDivElement>(null);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const [container, setContainer] = useState({ width: 0, height: 0 });
  const size = useMirrorStore((state) => state.size);
  const status = useMirrorStore((state) => state.status);
  const setCanvas = useMirrorStore((state) => state.setCanvas);
  const send = useMirrorStore((state) => state.send);

  const touching = useRef(false);
  const pendingMove = useRef<{ x: number; y: number } | null>(null);
  const moveFrame = useRef(0);
  const pressedKeys = useRef(new Set<number>());

  useLayoutEffect(() => {
    setCanvas(canvasRef.current);
    return () => setCanvas(null);
  }, [setCanvas]);

  useEffect(() => {
    const element = containerRef.current;
    if (!element) {
      return;
    }
    const observer = new ResizeObserver(([entry]) => {
      setContainer({ width: entry.contentRect.width, height: entry.contentRect.height });
    });
    observer.observe(element);
    return () => observer.disconnect();
  }, []);

  useEffect(() => {
    if (status === "streaming") {
      containerRef.current?.focus();
    }
  }, [status]);

  const display = fitInside(container, size);

  const point = (event: { clientX: number; clientY: number }) =>
    toVideoPoint(event.clientX, event.clientY, canvasRef.current!.getBoundingClientRect(), size);

  const sendTouch = (action: number, at: { x: number; y: number }, pressure: number) => {
    send({
      type: "touch",
      action,
      pointerId: POINTER_ID_FINGER,
      x: at.x,
      y: at.y,
      width: size.width,
      height: size.height,
      pressure,
      actionButton: 0,
      buttons: 0,
    });
  };

  const flushMove = () => {
    cancelAnimationFrame(moveFrame.current);
    moveFrame.current = 0;
    if (pendingMove.current) {
      sendTouch(ACTION_MOVE, pendingMove.current, 1);
      pendingMove.current = null;
    }
  };

  const pressKey = (keycode: number) => {
    for (const action of [ACTION_DOWN, ACTION_UP]) {
      send({ type: "keycode", action, keycode, repeat: 0, metastate: 0 });
    }
  };

  const onPointerDown = (event: PointerEvent<HTMLCanvasElement>) => {
    containerRef.current?.focus();
    if (event.button === BUTTON_LEFT) {
      event.currentTarget.setPointerCapture(event.pointerId);
      touching.current = true;
      sendTouch(ACTION_DOWN, point(event), 1);
    } else if (event.button === BUTTON_RIGHT) {
      send({ type: "backOrScreenOn", action: ACTION_DOWN });
      send({ type: "backOrScreenOn", action: ACTION_UP });
    } else if (event.button === BUTTON_MIDDLE) {
      event.preventDefault();
      pressKey(KEYCODE_HOME);
    }
  };

  const onPointerMove = (event: PointerEvent<HTMLCanvasElement>) => {
    if (!touching.current) {
      return;
    }
    pendingMove.current = point(event);
    if (!moveFrame.current) {
      moveFrame.current = requestAnimationFrame(flushMove);
    }
  };

  const endTouch = (event: PointerEvent<HTMLCanvasElement>, action: number) => {
    if (!touching.current) {
      return;
    }
    touching.current = false;
    flushMove();
    sendTouch(action, point(event), 0);
  };

  const onWheel = (event: WheelEvent<HTMLCanvasElement>) => {
    const at = point(event);
    send({
      type: "scroll",
      ...at,
      width: size.width,
      height: size.height,
      ...wheelToScroll(event.deltaX, event.deltaY),
      buttons: 0,
    });
  };

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    event.preventDefault();
    if (!event.repeat && needsTextInjection(event.key, event.ctrlKey, event.altKey)) {
      send({ type: "text", text: event.key });
      return;
    }
    const keycode = androidKeycode(event.code);
    if (keycode === null) {
      return;
    }
    pressedKeys.current.add(keycode);
    send({
      type: "keycode",
      action: ACTION_DOWN,
      keycode,
      repeat: event.repeat ? 1 : 0,
      metastate: androidMetaState({
        shiftKey: event.shiftKey,
        altKey: event.altKey,
        ctrlKey: event.ctrlKey,
        metaKey: event.metaKey,
        capsLock: event.getModifierState("CapsLock"),
      }),
    });
  };

  const onKeyUp = (event: KeyboardEvent<HTMLDivElement>) => {
    event.preventDefault();
    const keycode = androidKeycode(event.code);
    if (keycode !== null && pressedKeys.current.delete(keycode)) {
      send({ type: "keycode", action: ACTION_UP, keycode, repeat: 0, metastate: 0 });
    }
  };

  // Without this, a key held while the window loses focus would stay down on the phone.
  const releaseAllKeys = () => {
    for (const keycode of pressedKeys.current) {
      send({ type: "keycode", action: ACTION_UP, keycode, repeat: 0, metastate: 0 });
    }
    pressedKeys.current.clear();
  };

  return (
    <div
      ref={containerRef}
      tabIndex={0}
      aria-label="Android screen. Mouse and keyboard input go to the device."
      // Inline, because the global :focus-visible rule outranks Tailwind's layered utilities.
      style={{ outline: "none" }}
      className="flex size-full items-center justify-center rounded-lg"
      onKeyDown={onKeyDown}
      onKeyUp={onKeyUp}
      onBlur={releaseAllKeys}
    >
      <canvas
        ref={canvasRef}
        data-testid="mirror-canvas"
        style={{ width: display.width, height: display.height }}
        className="touch-none rounded-lg bg-black"
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={(event) => endTouch(event, ACTION_UP)}
        onPointerCancel={(event) => endTouch(event, ACTION_CANCEL)}
        onWheel={onWheel}
        onContextMenu={(event) => event.preventDefault()}
      />
    </div>
  );
}
