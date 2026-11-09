import type { TransferResult } from "@/types";

export function summarizeTransfer(results: TransferResult[]): string {
  if (results.length === 1) {
    const [{ name, action }] = results;
    return action === "installed" ? `Installed ${name}` : `Sent ${name} to Download`;
  }
  const installed = results.filter((result) => result.action === "installed").length;
  const sent = results.length - installed;
  return [installed && `Installed ${installed} apps`, sent && `Sent ${sent} files to Download`]
    .filter(Boolean)
    .join(", ");
}

export function randomToken(bytes: number): string {
  const values = crypto.getRandomValues(new Uint8Array(bytes));
  return [...values].map((value) => value.toString(16).padStart(2, "0")).join("");
}

/** The QR format Android's "Pair device with QR code" screen expects. */
export function pairingQrText(name: string, password: string): string {
  return `WIFI:T:ADB;S:${name};P:${password};;`;
}
