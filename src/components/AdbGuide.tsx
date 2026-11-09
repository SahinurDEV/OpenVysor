import { TriangleAlert } from "lucide-react";
import { Card } from "@/components/ui/card";

export function AdbGuide() {
  return (
    <Card className="flex gap-4 p-5">
      <TriangleAlert className="mt-0.5 size-5 shrink-0 text-warning" />
      <div className="space-y-2 select-text">
        <h2 className="font-semibold">ADB is not installed</h2>
        <p className="text-muted-foreground">
          OpenVysor talks to your phone through Android Debug Bridge. Install it either way below,
          and this message clears on its own within a few seconds.
        </p>
        <ul className="list-disc space-y-1 pl-5 text-muted-foreground">
          <li>
            Run <code className="text-foreground">winget install Google.PlatformTools</code>
          </li>
          <li>
            Or unzip Platform Tools from{" "}
            <span className="text-foreground">developer.android.com/tools/releases/platform-tools</span>{" "}
            and add the folder to PATH
          </li>
        </ul>
      </div>
    </Card>
  );
}
