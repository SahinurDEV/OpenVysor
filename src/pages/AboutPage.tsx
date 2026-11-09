import { Logo } from "@/components/Logo";
import { Card } from "@/components/ui/card";

export function AboutPage() {
  return (
    <div className="page-enter h-full overflow-y-auto p-8">
      <Card className="max-w-2xl space-y-4 p-6">
        <div className="flex items-center gap-4">
          <Logo size={44} />
          <div>
            <h1 className="font-display text-xl font-semibold">OpenVysor</h1>
            <p className="text-[13px] text-muted-foreground">Version 0.1.0 · Apache License 2.0</p>
          </div>
        </div>
        <p className="text-muted-foreground">
          Free, open-source Android mirroring and control for Windows. OpenVysor is an independent
          project and is not affiliated with or endorsed by the makers of Vysor.
        </p>
        <div>
          <h2 className="mb-1 font-semibold">Third-party software</h2>
          <p className="text-[13px] text-muted-foreground">
            Screen capture and input use scrcpy-server 4.1, Copyright © 2018 Genymobile and
            Copyright © 2018–2026 Romain Vimont, under the Apache License 2.0. Its license ships
            with this application.
          </p>
        </div>
      </Card>
    </div>
  );
}
