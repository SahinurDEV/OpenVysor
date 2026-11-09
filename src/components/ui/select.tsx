import type { ComponentProps } from "react";
import { cn } from "@/utils/cn";

export function Select({ className, ...props }: ComponentProps<"select">) {
  return (
    <select
      className={cn("h-8 rounded-lg border bg-card px-2 text-[13px] text-foreground", className)}
      {...props}
    />
  );
}
