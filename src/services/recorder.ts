import { api } from "@/services/api";

const FORMATS = [
  { mimeType: "video/mp4;codecs=avc1.640028,mp4a.40.2", extension: "mp4" },
  { mimeType: "video/mp4;codecs=avc1.640028", extension: "mp4" },
  { mimeType: "video/webm;codecs=vp9,opus", extension: "webm" },
  { mimeType: "video/webm", extension: "webm" },
];
const CHUNK_MILLISECONDS = 1000;
const FRAME_RATE = 30;
const VIDEO_BITS_PER_SECOND = 10_000_000;

export class ScreenRecorder {
  private chain: Promise<unknown> = Promise.resolve();

  private constructor(
    private readonly session: string,
    private readonly recorder: MediaRecorder,
    readonly path: string,
  ) {
    // Chunks are chained because an out-of-order write would corrupt the file.
    recorder.ondataavailable = (event) => {
      this.chain = this.chain
        .then(async () => api.recordingChunk(session, new Uint8Array(await event.data.arrayBuffer())))
        .catch((error) => console.error("recording chunk failed", error));
    };
    recorder.start(CHUNK_MILLISECONDS);
  }

  static async start(
    session: string,
    canvas: HTMLCanvasElement,
    audio: MediaStream | null,
  ): Promise<ScreenRecorder> {
    const format = FORMATS.find((candidate) => MediaRecorder.isTypeSupported(candidate.mimeType));
    if (!format) {
      throw new Error("This WebView2 runtime cannot record video.");
    }
    const stream = canvas.captureStream(FRAME_RATE);
    audio?.getAudioTracks().forEach((track) => stream.addTrack(track));
    const recorder = new MediaRecorder(stream, {
      mimeType: format.mimeType,
      videoBitsPerSecond: VIDEO_BITS_PER_SECOND,
    });
    const path = await api.recordingStart(session, format.extension);
    return new ScreenRecorder(session, recorder, path);
  }

  async stop(): Promise<string> {
    if (this.recorder.state !== "inactive") {
      await new Promise((resolve) => {
        this.recorder.onstop = resolve;
        this.recorder.stop();
      });
    }
    await this.chain;
    await api.recordingStop(this.session);
    return this.path;
  }
}
