const SAMPLE_RATE = 48000;
/** Scheduled slightly ahead so small network gaps do not cause clicks. */
const START_LEAD_SECONDS = 0.04;
/** Beyond this much queued audio, playback restarts close to live. */
const MAX_QUEUED_SECONDS = 0.25;

export class AudioPlayer {
  private readonly context = new AudioContext({ latencyHint: "interactive", sampleRate: SAMPLE_RATE });
  private readonly output = this.context.createMediaStreamDestination();
  private readonly decoder: AudioDecoder;
  private nextStart = 0;
  private reportedPlaying = false;

  constructor(
    description: Uint8Array<ArrayBuffer>,
    private readonly onPlaying: () => void,
  ) {
    this.decoder = new AudioDecoder({
      output: (data) => this.play(data),
      error: (error) => console.error("audio decoder error", error),
    });
    this.decoder.configure({ codec: "opus", sampleRate: SAMPLE_RATE, numberOfChannels: 2, description });
    void this.context.resume();
  }

  /** The decoded audio as a stream, so recordings can include it. */
  get stream(): MediaStream {
    return this.output.stream;
  }

  decode(pts: number, data: Uint8Array<ArrayBuffer>): void {
    if (this.decoder.state === "configured") {
      this.decoder.decode(new EncodedAudioChunk({ type: "key", timestamp: pts, data }));
    }
  }

  private play(data: AudioData): void {
    const buffer = this.context.createBuffer(data.numberOfChannels, data.numberOfFrames, data.sampleRate);
    for (let channel = 0; channel < data.numberOfChannels; channel += 1) {
      data.copyTo(buffer.getChannelData(channel), { planeIndex: channel, format: "f32-planar" });
    }
    data.close();

    const source = this.context.createBufferSource();
    source.buffer = buffer;
    source.connect(this.context.destination);
    source.connect(this.output);

    const now = this.context.currentTime;
    if (this.nextStart < now || this.nextStart > now + MAX_QUEUED_SECONDS) {
      this.nextStart = now + START_LEAD_SECONDS;
    }
    source.start(this.nextStart);
    this.nextStart += buffer.duration;

    if (!this.reportedPlaying) {
      this.reportedPlaying = true;
      this.onPlaying();
    }
  }

  close(): void {
    if (this.decoder.state !== "closed") {
      this.decoder.close();
    }
    void this.context.close();
  }
}
