import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
} from "@/components/ui/dialog";
import { Badge } from "@/components/ui/badge";
import { useTraces } from "@/hooks/use-traces";
import type { TraceAttempt, TraceRecord } from "@/types/api";

interface CredentialFailuresDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  credentialId: number;
  email?: string;
}

/** Failedcategory → Chinese label + Badge color */
function outcomeStyle(outcome: string | null): {
  label: string;
  variant: "destructive" | "warning" | "outline" | "secondary";
} {
  switch (outcome) {
    case "quota_exhausted":
      return { label: "Quota exhausted", variant: "warning" };
    case "account_throttled":
      return { label: "Account throttle", variant: "warning" };
    case "auth_failed":
      return { label: "Authentication failed", variant: "destructive" };
    case "transient":
      return { label: "Transient error", variant: "outline" };
    case "network_error":
      return { label: "Network error", variant: "destructive" };
    case "bad_request":
      return { label: "Request error", variant: "destructive" };
    case "stream_interrupted":
      return { label: "Stream interrupted", variant: "warning" };
    default:
      return { label: outcome || "Unknown", variant: "secondary" };
  }
}

function formatTime(ts: string): string {
  const d = new Date(ts);
  if (isNaN(d.getTime())) return ts;
  return d.toLocaleString("zh-CN", { hour12: false });
}

function keySourceLabel(rec: TraceRecord): string {
  return rec.keyName ?? `#${rec.keyId}`;
}

export function CredentialFailuresDialog({
  open,
  onOpenChange,
  credentialId,
  email,
}: CredentialFailuresDialogProps) {
  const { data, isLoading } = useTraces(
    { failedAttemptCredentialId: credentialId, limit: 50 },
    open,
  );
  const records = data?.records ?? [];
  // flatten:the sameRequestthe one insideCredentialFaileda fewjumpthenShowfewitems(byTimereverse order)
  const failedHops = records.flatMap((rec) =>
    rec.attempts
      .filter((a) => a.credentialId === credentialId && a.outcome !== "success")
      .map((a) => ({ rec, attempt: a })),
  );

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-2xl">
        <DialogHeader>
          <DialogTitle>Failure log details</DialogTitle>
          <DialogDescription>
            {email || `Credential #${credentialId}`} Recent failure records (up to 50 requests)
          </DialogDescription>
        </DialogHeader>
        <div className="max-h-[60vh] space-y-2 overflow-y-auto">
          {isLoading ? (
            <div className="py-6 text-center text-sm text-muted-foreground">
              Loading…
            </div>
          ) : failedHops.length === 0 ? (
            <div className="py-6 text-center text-sm text-muted-foreground">
              This credential has no failure records yet (trace off or no recent failures).
            </div>
          ) : (
            failedHops.map(({ rec, attempt }) => (
              <FailureRow
                key={`${rec.traceId}-${attempt.attempt}`}
                rec={rec}
                attempt={attempt}
              />
            ))
          )}
        </div>
      </DialogContent>
    </Dialog>
  );
}

/** singlejumpFailed:show thisCredentialsometimesFailedof outcome / HTTP / Errorbody */
function FailureRow({
  rec,
  attempt,
}: {
  rec: TraceRecord;
  attempt: TraceAttempt;
}) {
  const style = outcomeStyle(attempt.outcome);
  // wholeitems trace whether afterwardSuccessdone(use anotherofCredentialrecover)
  const traceRecovered = rec.finalStatus === "success";
  return (
    <div className="rounded-lg border border-border/50 bg-secondary/30 p-3">
      <div className="flex flex-wrap items-center gap-2 text-[13px]">
        <span className="tabular-nums text-muted-foreground">
          {formatTime(rec.ts)}
        </span>
        <Badge variant="secondary">{keySourceLabel(rec)}</Badge>
        <Badge variant={style.variant}>{style.label}</Badge>
        {attempt.httpStatus != null && (
          <span className="font-mono text-muted-foreground">
            HTTP {attempt.httpStatus}
          </span>
        )}
        {rec.totalAttempts > 1 && (
          <span className="text-[12px] text-muted-foreground">
            No. {attempt.attempt + 1}/{rec.totalAttempts} jump
          </span>
        )}
        {traceRecovered && (
          <Badge variant="outline">This request eventually succeeded via another credential</Badge>
        )}
        {rec.finalStatus === "interrupted" && (
          <Badge variant="warning">Interrupted</Badge>
        )}
        <span className="ml-auto text-[12px] text-muted-foreground">
          {rec.model}
        </span>
      </div>
      {attempt.errorSnippet && (
        <pre className="mt-2 max-h-32 overflow-auto whitespace-pre-wrap break-all rounded-md bg-background/60 p-2 font-mono text-[11px] text-muted-foreground">
          {attempt.errorSnippet}
        </pre>
      )}
    </div>
  );
}
