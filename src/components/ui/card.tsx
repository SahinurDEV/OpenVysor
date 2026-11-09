import type { ComponentProps } from "react";
import { cn } from "@/utils/cn";

export function Card({ className, ...props }: ComponentProps<"div">) {
  return <div className={cn("rounded-xl border bg-card", className)} {...props} />;
}
