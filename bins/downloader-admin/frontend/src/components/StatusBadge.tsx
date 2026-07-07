import { Badge } from "@/components/ui/badge";
import type { RequestStatusType } from "@/lib/api";

const MAP: Record<
  RequestStatusType,
  { label: string; variant: "default" | "secondary" | "destructive" | "success" | "warning" }
> = {
  pending: { label: "Pending", variant: "secondary" },
  inProgress: { label: "In progress", variant: "warning" },
  done: { label: "Done", variant: "success" },
  failed: { label: "Failed", variant: "destructive" },
};

export function StatusBadge({ status }: { status: RequestStatusType }) {
  const { label, variant } = MAP[status] ?? {
    label: status,
    variant: "outline" as const,
  };
  return <Badge variant={variant}>{label}</Badge>;
}
