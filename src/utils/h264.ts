const NAL_TYPE_SPS = 7;

/** Builds an `avc1.PPCCLL` codec string from the SPS in an Annex B config packet. */
export function h264CodecString(config: Uint8Array): string | null {
  for (let i = 0; i + 6 < config.length; i += 1) {
    if (config[i] !== 0 || config[i + 1] !== 0 || config[i + 2] !== 1) {
      continue;
    }
    if ((config[i + 3] & 0x1f) === NAL_TYPE_SPS) {
      const hex = [config[i + 4], config[i + 5], config[i + 6]]
        .map((byte) => byte.toString(16).padStart(2, "0"))
        .join("");
      return `avc1.${hex}`;
    }
  }
  return null;
}
