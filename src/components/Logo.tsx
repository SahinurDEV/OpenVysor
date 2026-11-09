export function Logo({ size = 28 }: { size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 32 32" aria-hidden="true">
      <rect width="32" height="32" rx="8" fill="#3ddc97" />
      <rect x="10.5" y="5.5" width="11" height="21" rx="2.5" fill="none" stroke="#06281b" strokeWidth="2" />
      <path d="M14 13.2v5.6l4.8-2.8z" fill="#06281b" />
    </svg>
  );
}
