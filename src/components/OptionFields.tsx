import { Select } from "@/components/ui/select";
import type { MirrorOptions } from "@/types";

const RESOLUTIONS = [
  { value: 0, label: "Native" },
  { value: 1920, label: "1920p" },
  { value: 1600, label: "1600p" },
  { value: 1280, label: "1280p" },
  { value: 1024, label: "1024p" },
  { value: 800, label: "800p" },
];
const BIT_RATES = [2, 4, 8, 16, 24, 32].map((mbps) => ({
  value: mbps * 1_000_000,
  label: `${mbps} Mbps`,
}));
const FRAME_RATES = [30, 60, 90, 120].map((fps) => ({ value: fps, label: `${fps} FPS` }));

interface FieldProps {
  label: string;
  value: number;
  choices: { value: number; label: string }[];
  disabled?: boolean;
  onChange: (value: number) => void;
}

function Field({ label, value, choices, disabled, onChange }: FieldProps) {
  return (
    <Select
      aria-label={label}
      title={label}
      value={value}
      disabled={disabled}
      onChange={(event) => onChange(Number(event.target.value))}
    >
      {choices.map((choice) => (
        <option key={choice.value} value={choice.value}>
          {choice.label}
        </option>
      ))}
    </Select>
  );
}

export function OptionFields({
  options,
  disabled,
  onChange,
}: {
  options: MirrorOptions;
  disabled?: boolean;
  onChange: (options: MirrorOptions) => void;
}) {
  return (
    <>
      <Field
        label="Resolution"
        value={options.maxSize}
        choices={RESOLUTIONS}
        disabled={disabled}
        onChange={(maxSize) => onChange({ ...options, maxSize })}
      />
      <Field
        label="Bitrate"
        value={options.bitRate}
        choices={BIT_RATES}
        disabled={disabled}
        onChange={(bitRate) => onChange({ ...options, bitRate })}
      />
      <Field
        label="Frame rate"
        value={options.maxFps}
        choices={FRAME_RATES}
        disabled={disabled}
        onChange={(maxFps) => onChange({ ...options, maxFps })}
      />
    </>
  );
}
