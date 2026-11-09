import { AudioPlayer } from "@/services/audioPlayer";
import { h264CodecString } from "@/utils/h264";

const PACKET_SESSION = 0;
const PACKET_CONFIG = 1;
const PACKET_KEY_FRAME = 2;
const PACKET_AUDIO_CONFIG = 4;
const PACKET_AUDIO_FRAME = 5;
const HEADER_SIZE = 9;
/** Beyond this many queued frames the UI is behind, so skip to the next key frame. */
const MAX_DECODE_QUEUE = 6;

export interface ParsedPacket {
  kind: number;
  pts: number;
  payload: Uint8Array<ArrayBuffer>;
}

export function parsePacket(buffer: ArrayBuffer): ParsedPacket {
  const view = new DataView(buffer);
  return {
    kind: view.getUint8(0),
    pts: Number(view.getBigUint64(1)),
    payload: new Uint8Array(buffer, HEADER_SIZE),
  };
}

export interface DecoderCallbacks {
  onSize: (width: number, height: number) => void;
  onFps: (fps: number) => void;
  onError: (message: string) => void;
  onAudioPlaying: () => void;
  requestKeyFrame: () => void;
}

export class MirrorDecoder {
  private decoder: VideoDecoder | null = null;
  private config: VideoDecoderConfig | null = null;
  private waitingForKeyFrame = true;
  private pendingFrame: VideoFrame | null = null;
  private animationFrame = 0;
  private framesThisSecond = 0;
  private fpsTimer: ReturnType<typeof setInterval>;
  private closed = false;
  private audio: AudioPlayer | null = null;
  private readonly context: CanvasRenderingContext2D;

  constructor(
    private readonly canvas: HTMLCanvasElement,
    private readonly callbacks: DecoderCallbacks,
  ) {
    const context = canvas.getContext("2d", { alpha: false, desynchronized: true });
    if (!context) {
      throw new Error("Could not create a canvas context");
    }
    this.context = context;
    this.fpsTimer = setInterval(() => {
      this.callbacks.onFps(this.framesThisSecond);
      this.framesThisSecond = 0;
    }, 1000);
  }

  handlePacket(buffer: ArrayBuffer): void {
    if (this.closed) {
      return;
    }
    const { kind, pts, payload } = parsePacket(buffer);
    if (kind === PACKET_SESSION) {
      const view = new DataView(payload.buffer, payload.byteOffset);
      this.callbacks.onSize(view.getUint32(0), view.getUint32(4));
    } else if (kind === PACKET_CONFIG) {
      this.configure(payload);
    } else if (kind === PACKET_AUDIO_CONFIG) {
      this.audio?.close();
      this.audio = new AudioPlayer(payload, this.callbacks.onAudioPlaying);
    } else if (kind === PACKET_AUDIO_FRAME) {
      this.audio?.decode(pts, payload);
    } else {
      this.decode(kind === PACKET_KEY_FRAME, pts, payload);
    }
  }

  get audioStream(): MediaStream | null {
    return this.audio?.stream ?? null;
  }

  private configure(payload: Uint8Array): void {
    const codec = h264CodecString(payload);
    if (!codec) {
      this.callbacks.onError("The device sent an unreadable H.264 configuration.");
      return;
    }
    this.config = { codec, optimizeForLatency: true };
    this.createDecoder();
  }

  private createDecoder(): void {
    if (!this.config) {
      return;
    }
    if (this.decoder && this.decoder.state !== "closed") {
      this.decoder.close();
    }
    this.decoder = new VideoDecoder({
      output: (frame) => this.onFrame(frame),
      error: (error) => this.onDecoderError(error),
    });
    this.decoder.configure(this.config);
    this.waitingForKeyFrame = true;
  }

  private decode(isKey: boolean, pts: number, data: Uint8Array<ArrayBuffer>): void {
    const decoder = this.decoder;
    if (!decoder || decoder.state !== "configured") {
      return;
    }
    if (this.waitingForKeyFrame && !isKey) {
      return;
    }
    if (!isKey && decoder.decodeQueueSize > MAX_DECODE_QUEUE) {
      this.waitingForKeyFrame = true;
      this.callbacks.requestKeyFrame();
      return;
    }
    this.waitingForKeyFrame = false;
    decoder.decode(new EncodedVideoChunk({ type: isKey ? "key" : "delta", timestamp: pts, data }));
  }

  private onDecoderError(error: DOMException): void {
    if (this.closed) {
      return;
    }
    console.error("video decoder error", error);
    // A decoder that failed is closed by the browser, so build a new one.
    try {
      this.createDecoder();
      this.callbacks.requestKeyFrame();
    } catch (cause) {
      this.callbacks.onError(`The video decoder failed: ${String(cause)}`);
    }
  }

  private onFrame(frame: VideoFrame): void {
    if (this.closed) {
      frame.close();
      return;
    }
    this.framesThisSecond += 1;
    this.pendingFrame?.close();
    this.pendingFrame = frame;
    if (!this.animationFrame) {
      this.animationFrame = requestAnimationFrame(() => this.paint());
    }
  }

  private paint(): void {
    this.animationFrame = 0;
    const frame = this.pendingFrame;
    if (!frame) {
      return;
    }
    this.pendingFrame = null;
    if (this.canvas.width !== frame.displayWidth || this.canvas.height !== frame.displayHeight) {
      this.canvas.width = frame.displayWidth;
      this.canvas.height = frame.displayHeight;
    }
    this.context.drawImage(frame, 0, 0);
    frame.close();
  }

  close(): void {
    this.closed = true;
    clearInterval(this.fpsTimer);
    if (this.animationFrame) {
      cancelAnimationFrame(this.animationFrame);
    }
    this.pendingFrame?.close();
    this.pendingFrame = null;
    this.audio?.close();
    if (this.decoder && this.decoder.state !== "closed") {
      this.decoder.close();
    }
  }
}
