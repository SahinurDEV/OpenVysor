import { useEffect, useRef, useState } from "react";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { LoaderCircle, Upload } from "lucide-react";
import { api, errorMessage } from "@/services/api";
import { summarizeTransfer } from "@/utils/transfer";

const NOTICE_MILLISECONDS = 6000;

/** Drop an APK to install it, or any other file to copy it to the phone. */
export function DropOverlay({ serial, enabled }: { serial: string | null; enabled: boolean }) {
  const [dragging, setDragging] = useState(false);
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  const timer = useRef<ReturnType<typeof setTimeout>>(undefined);

  useEffect(() => {
    if (!serial || !enabled) {
      setDragging(false);
      return;
    }
    const subscription = getCurrentWebview().onDragDropEvent(({ payload }) => {
      if (payload.type === "enter" || payload.type === "over") {
        setDragging(true);
      } else if (payload.type === "leave") {
        setDragging(false);
      } else {
        setDragging(false);
        setBusy(true);
        setNotice(null);
        clearTimeout(timer.current);
        api
          .sendFiles(serial, payload.paths)
          .then((results) => setNotice(summarizeTransfer(results)))
          .catch((error) => setNotice(errorMessage(error)))
          .finally(() => {
            setBusy(false);
            timer.current = setTimeout(() => setNotice(null), NOTICE_MILLISECONDS);
          });
      }
    });
    return () => void subscription.then((unlisten) => unlisten());
  }, [serial, enabled]);

  return (
    <>
      {dragging && (
        <div className="pointer-events-none absolute inset-2 flex flex-col items-center justify-center gap-2 rounded-xl border-2 border-dashed border-primary bg-black/70 p-4 text-center text-white">
          <Upload className="size-7 text-primary" />
          <p className="text-[13px] font-medium">Drop to install an APK or send files</p>
        </div>
      )}
      {(busy || notice) && (
        <div
          data-testid="transfer-status"
          className="absolute bottom-3 left-1/2 flex max-w-[90%] -translate-x-1/2 items-center gap-2 rounded-lg border bg-card px-3 py-2 text-[13px] text-foreground shadow-lg select-text"
        >
          {busy && <LoaderCircle className="size-4 shrink-0 animate-spin text-primary" />}
          <span className="truncate">{busy ? "Working…" : notice}</span>
        </div>
      )}
    </>
  );
}
