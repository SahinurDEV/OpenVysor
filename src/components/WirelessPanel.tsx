import { useEffect, useMemo, useState } from "react";
import type { FormEvent } from "react";
import { QRCodeSVG } from "qrcode.react";
import { CircleCheck, LoaderCircle } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { api, errorMessage } from "@/services/api";
import { pairingQrText, randomToken } from "@/utils/transfer";

type Mode = "qr" | "code" | "address";

const MODES: { id: Mode; label: string }[] = [
  { id: "qr", label: "QR code" },
  { id: "code", label: "Pairing code" },
  { id: "address", label: "IP address" },
];
const POLL_MILLISECONDS = 1500;
const inputClass = "h-9 w-full rounded-lg border bg-card px-3 text-[13px] select-text";

function QrPairing() {
  const { name, password } = useMemo(
    () => ({ name: `studio-${randomToken(4)}`, password: randomToken(8) }),
    [],
  );
  const [status, setStatus] = useState<"waiting" | "paired">("waiting");
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let active = true;
    let timer: ReturnType<typeof setTimeout>;
    const poll = async () => {
      try {
        if (await api.wirelessPollQr(name, password)) {
          if (active) setStatus("paired");
          return;
        }
        if (active) setError(null);
      } catch (cause) {
        if (active) setError(errorMessage(cause));
      }
      if (active) timer = setTimeout(poll, POLL_MILLISECONDS);
    };
    void poll();
    return () => {
      active = false;
      clearTimeout(timer);
    };
  }, [name, password]);

  return (
    <div className="flex flex-wrap items-center gap-6">
      <div className="rounded-lg bg-white p-3">
        <QRCodeSVG value={pairingQrText(name, password)} size={168} />
      </div>
      <div className="min-w-60 flex-1 space-y-2 text-[13px]">
        <ol className="list-decimal space-y-1 pl-5 text-muted-foreground">
          <li>Connect the phone and this computer to the same Wi-Fi</li>
          <li>On the phone, open Developer options, then Wireless debugging</li>
          <li>Tap “Pair device with QR code” and scan this code</li>
        </ol>
        {status === "waiting" ? (
          <p className="flex items-center gap-2 text-muted-foreground">
            <LoaderCircle className="size-4 animate-spin" />
            Waiting for the phone…
          </p>
        ) : (
          <p className="flex items-center gap-2 font-medium text-success">
            <CircleCheck className="size-4" />
            Paired. The phone will appear in your devices.
          </p>
        )}
        {error && <p className="text-danger select-text">{error}</p>}
      </div>
    </div>
  );
}

function AddressForm({ mode }: { mode: "code" | "address" }) {
  const [address, setAddress] = useState("");
  const [code, setCode] = useState("");
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState<{ ok: boolean; text: string } | null>(null);

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    setBusy(true);
    setResult(null);
    try {
      if (mode === "code") {
        await api.wirelessPair(address.trim(), code.trim());
        setResult({ ok: true, text: "Paired. Now connect using the IP address tab." });
      } else {
        setResult({ ok: true, text: await api.wirelessConnect(address.trim()) });
      }
    } catch (cause) {
      setResult({ ok: false, text: errorMessage(cause) });
    } finally {
      setBusy(false);
    }
  };

  return (
    <form onSubmit={submit} className="max-w-md space-y-3 text-[13px]">
      <p className="text-muted-foreground">
        {mode === "code"
          ? "On the phone, open Wireless debugging, then “Pair device with pairing code”."
          : "Use the IP address and port shown at the top of Wireless debugging."}
      </p>
      <input
        className={inputClass}
        aria-label="IP address and port"
        placeholder="192.168.1.20:37000"
        value={address}
        onChange={(event) => setAddress(event.target.value)}
      />
      {mode === "code" && (
        <input
          className={inputClass}
          aria-label="Pairing code"
          placeholder="6-digit code"
          inputMode="numeric"
          value={code}
          onChange={(event) => setCode(event.target.value)}
        />
      )}
      <Button variant="primary" size="sm" type="submit" disabled={busy || !address.trim()}>
        {busy && <LoaderCircle className="animate-spin" />}
        {mode === "code" ? "Pair" : "Connect"}
      </Button>
      {result && <p className={`select-text ${result.ok ? "text-success" : "text-danger"}`}>{result.text}</p>}
    </form>
  );
}

export function WirelessPanel() {
  const [mode, setMode] = useState<Mode>("qr");

  return (
    <Card className="mb-6 space-y-4 p-5">
      <div className="flex gap-1">
        {MODES.map(({ id, label }) => (
          <Button
            key={id}
            size="sm"
            variant={mode === id ? "secondary" : "ghost"}
            aria-pressed={mode === id}
            onClick={() => setMode(id)}
          >
            {label}
          </Button>
        ))}
      </div>
      {mode === "qr" ? <QrPairing /> : <AddressForm key={mode} mode={mode} />}
    </Card>
  );
}
