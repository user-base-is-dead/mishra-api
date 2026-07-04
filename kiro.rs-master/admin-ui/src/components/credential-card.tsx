import { useState, useEffect, useCallback } from "react";
import { toast } from "sonner";
import {
  RefreshCw,
  GripVertical,
  Trash2,
  Loader2,
  Pencil,
  LogIn,
  MoreHorizontal,
  RotateCcw,
  Zap,
  ZapOff,
  Clock,
  ScrollText,
  Boxes,
  Wallet,
} from "lucide-react";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Switch } from "@/components/ui/switch";
import { Input } from "@/components/ui/input";
import { Checkbox } from "@/components/ui/checkbox";
import { Progress } from "@/components/ui/progress";
import {
  DropdownMenu,
  DropdownMenuTrigger,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
} from "@/components/ui/dropdown-menu";
import { SubscriptionBadge } from "@/components/subscription-badge";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import type { CredentialStatusItem, BalanceResponse } from "@/types/api";
import { maskProxyUrl, extractErrorMessage, overageFailureMessage } from "@/lib/utils";
import {
  useSetDisabled,
  useSetPriority,
  useResetFailure,
  useDeleteCredential,
  useForceRefreshToken,
  useResetSuccessCount,
  useClearThrottle,
} from "@/hooks/use-credentials";
import { setCredentialOverage } from "@/api/credentials";
import { useQueryClient } from "@tanstack/react-query";
import { useSortable } from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";
import { EditCredentialDialog } from "@/components/edit-credential-dialog";
import { UpdateTokenDialog } from "@/components/update-token-dialog";
import { ReloginDialog } from "@/components/relogin-dialog";
import { CredentialFailuresDialog } from "@/components/credential-failures-dialog";
import { AvailableModelsDialog } from "@/components/available-models-dialog";

interface CredentialCardProps {
  credential: CredentialStatusItem;
  selected: boolean;
  onToggleSelect: () => void;
  balance: BalanceResponse | null;
  loadingBalance: boolean;
  onRefreshBalance: () => void;
  /** theCredentialofFailedcount by category(from trace Aggregate);fall back when no data totalFailureCount */
  failureStats?: { auth: number; throttle: number; other: number };
  /** display form:Card(Default)orcompactListrow */
  view?: "card" | "list";
}

function formatLastUsed(lastUsedAt: string | null): string {
  if (!lastUsedAt) return "Never used";
  const date = new Date(lastUsedAt);
  const diff = Date.now() - date.getTime();
  if (diff < 0) return "Just now";
  const s = Math.floor(diff / 1000);
  if (s < 60) return `${s} seconds ago`;
  const m = Math.floor(s / 60);
  if (m < 60) return `${m} minutes ago`;
  const h = Math.floor(m / 60);
  if (h < 24) return `${h} hours ago`;
  return `${Math.floor(h / 24)} days ago`;
}

function formatNumber(n: number): string {
  return n.toLocaleString("zh-CN", {
    minimumFractionDigits: 2,
    maximumFractionDigits: 2,
  });
}

function formatResetDate(ts: number | null): string {
  if (!ts) return "Unknown";
  return new Date(ts * 1000).toLocaleString("zh-CN");
}

/** putsecondsFormatconvertas `mm:ss` or `hh:mm:ss` */
function formatThrottleCountdown(secs: number): string {
  const total = Math.max(0, Math.floor(secs));
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  const pad = (n: number) => String(n).padStart(2, "0");
  return h > 0 ? `${h}:${pad(m)}:${pad(s)}` : `${pad(m)}:${pad(s)}`;
}

/**
 * compactOverageStatuspill — shown alongside the subscription badge,does not take a full row
 * three state:Enabled(solid green color)/ Not enabled(neutral thin border)/ notSupported(gray dashed border small text)
 */
function OverageStatusPill({ balance }: { balance: BalanceResponse }) {
  const cap = balance.overageCapable;
  const on = balance.overageEnabled === true;

  // notSupportedofsubscription:heavily de emphasized
  if (cap === false) return null;

  if (on) {
    return (
      <span
        className="inline-flex items-center gap-1 rounded-full bg-emerald-500/15 px-2 h-6 text-[11px] font-medium text-emerald-700 dark:text-emerald-400"
        title="This account has overage enabled"
      >
        <Zap className="h-3 w-3" />
        Overage
      </span>
    );
  }

  if (cap === true) {
    return (
      <span
        className="inline-flex items-center gap-1 rounded-full border border-amber-500/40 bg-transparent px-2 h-6 text-[11px] font-medium text-amber-600 dark:text-amber-400"
        title="This account supports overage but it is not enabled"
      >
        <ZapOff className="h-3 w-3" />
        Not enabled
      </span>
    );
  }

  // Unknown:subtle gray,hover see the raw value
  return (
    <span
      className="inline-flex items-center gap-1 rounded-full border border-dashed border-border/60 bg-transparent px-2 h-6 text-[11px] text-muted-foreground"
      title={
        balance.overageCapabilityRaw
          ? `overageCapability = ${balance.overageCapabilityRaw}`
          : "The upstream did not return overageCapability"
      }
    >
      <ZapOff className="h-3 w-3" />
      Unknown
    </span>
  );
}

/**
 * putbackendBackof disabledReason stringmappingasmore intuitiveofChinese badge
 * (color/text/sort weight,the earlier the more prominent)
 */
function getDisabledReasonStyle(reason?: string | null): {
  label: string;
  variant: "destructive" | "warning" | "outline" | "secondary";
} | null {
  if (!reason) return null;
  switch (reason) {
    case "QuotaExceeded":
      return { label: "Over quota", variant: "warning" };
    case "TooManyFailures":
      return { label: "Too many failures", variant: "destructive" };
    case "TooManyRefreshFailures":
      return { label: "Too many refresh failures", variant: "destructive" };
    case "InvalidRefreshToken":
      return { label: "Token Invalid", variant: "destructive" };
    case "InvalidConfig":
      return { label: "Invalid config", variant: "destructive" };
    case "Manual":
      return { label: "Manually disabled", variant: "secondary" };
    default:
      return { label: reason, variant: "outline" };
  }
}

export function CredentialCard({
  credential,
  selected,
  onToggleSelect,
  balance,
  loadingBalance,
  onRefreshBalance,
  failureStats,
  view = "card",
}: CredentialCardProps) {
  const [editingPriority, setEditingPriority] = useState(false);
  const [priorityValue, setPriorityValue] = useState(
    String(credential.priority),
  );
  const [showDeleteDialog, setShowDeleteDialog] = useState(false);
  const [showEditDialog, setShowEditDialog] = useState(false);
  const [showUpdateTokenDialog, setShowUpdateTokenDialog] = useState(false);
  const [showReloginDialog, setShowReloginDialog] = useState(false);
  const [showFailuresDialog, setShowFailuresDialog] = useState(false);
  const [showModelsDialog, setShowModelsDialog] = useState(false);

  const setDisabled = useSetDisabled();
  const setPriority = useSetPriority();
  const resetFailure = useResetFailure();
  const deleteCredential = useDeleteCredential();
  const forceRefresh = useForceRefreshToken();
  const resetSuccess = useResetSuccessCount();
  const clearThrottle = useClearThrottle();
  const queryClient = useQueryClient();

  // drag to reorder:handle triggers,the whole card moves with the drag
  const {
    attributes,
    listeners,
    setNodeRef,
    setActivatorNodeRef,
    transform,
    transition,
    isDragging,
  } = useSortable({ id: credential.id });
  const dragStyle: React.CSSProperties = {
    transform: CSS.Transform.toString(transform),
    // turn off transitions during drag,avoid Card base classof transition-all puteach frame transform animating causes"does not follow the cursor";
    // notkeep the drag state dnd-kit ofreturn transition.
    transition: isDragging ? "none" : transition,
    zIndex: isDragging ? 20 : undefined,
  };

  // backendCooldownRemainingthe seconds willin 30s stale between fetch intervals,for local use setInterval decrease naturally so the countdown stays continuous.
  const [throttleRemaining, setThrottleRemaining] = useState<number>(
    credential.throttledRemainingSecs ?? 0,
  );
  useEffect(() => {
    setThrottleRemaining(credential.throttledRemainingSecs ?? 0);
  }, [credential.throttledRemainingSecs]);
  useEffect(() => {
    if (throttleRemaining <= 0) return;
    const t = window.setInterval(() => {
      setThrottleRemaining((v) => (v > 0 ? v - 1 : 0));
    }, 1000);
    return () => window.clearInterval(t);
  }, [throttleRemaining]);
  const handleClearThrottle = useCallback(() => {
    clearThrottle.mutate(credential.id, {
      onSuccess: (res) => {
        setThrottleRemaining(0);
        toast.success(res.message);
      },
      onError: (err) => toast.error("Release failed: " + extractErrorMessage(err)),
    });
  }, [clearThrottle, credential.id]);
  const [overageBusy, setOverageBusy] = useState(false);
  const handleSetOverage = async (enabled: boolean) => {
    setOverageBusy(true);
    try {
      await setCredentialOverage(credential.id, enabled);
      toast.success(enabled ? "Overage enabled" : "Overage disabled");
      queryClient.invalidateQueries({ queryKey: ["credentials"] });
    } catch (err) {
      toast.error(
        (enabled ? "Enable" : "Close") +
          "Overage failed: " +
          overageFailureMessage(extractErrorMessage(err)),
      );
    } finally {
      setOverageBusy(false);
    }
  };

  const handleToggleDisabled = () => {
    // currentasDisablestate → thistimesActionsis“Enable”,EnableSuccessthen alsoRefreshonetimesBalance
    const willEnable = credential.disabled;
    setDisabled.mutate(
      { id: credential.id, disabled: !credential.disabled },
      {
        onSuccess: (res) => {
          toast.success(res.message);
          if (willEnable) onRefreshBalance();
        },
        onError: (err) => toast.error("Operation failed: " + (err as Error).message),
      },
    );
  };

  const handlePriorityChange = () => {
    const np = parseInt(priorityValue, 10);
    if (isNaN(np) || np < 0) {
      toast.error("Priority must be a non-negative integer");
      return;
    }
    setPriority.mutate(
      { id: credential.id, priority: np },
      {
        onSuccess: (res) => {
          toast.success(res.message);
          setEditingPriority(false);
        },
        onError: (err) => toast.error("Operation failed: " + (err as Error).message),
      },
    );
  };

  const handleReset = () =>
    resetFailure.mutate(credential.id, {
      onSuccess: (res) => toast.success(res.message),
      onError: (err) => toast.error("Operation failed: " + (err as Error).message),
    });

  const handleForceRefresh = () =>
    forceRefresh.mutate(credential.id, {
      onSuccess: (res) => toast.success(res.message),
      onError: (err) => toast.error("Refresh failed: " + extractErrorMessage(err)),
    });

  const handleResetSuccess = () =>
    resetSuccess.mutate(credential.id, {
      onSuccess: (res) => toast.success(res.message),
      onError: (err) => toast.error("Reset failed: " + (err as Error).message),
    });

  const handleDelete = () => {
    deleteCredential.mutate(credential.id, {
      onSuccess: (res) => {
        toast.success(res.message);
        setShowDeleteDialog(false);
      },
      onError: (err) => toast.error("Delete failed: " + (err as Error).message),
    });
  };

  const authLabel = (() => {
    if (credential.authMethod === "api_key") return "API Key";
    const provider = credential.provider?.toLowerCase();
    if (credential.authMethod === "social") {
      if (provider === "github") return "GitHub";
      if (provider === "google") return "Google";
      return "Social";
    }
    if (credential.authMethod === "idc") {
      if (provider === "enterprise") return "Enterprise";
      if (provider === "iam_sso") return "IAM SSO";
      if (provider === "builderid") return "Builder ID";
      return "IdC";
    }
    return credential.authMethod;
  })();

  const isQuotaExceeded = balance
    ? balance.remaining <= 0 || balance.usagePercentage >= 100
    : false;

  const disabledByQuota =
    credential.disabled && credential.disabledReason === "QuotaExceeded";
  const reasonStyle = getDisabledReasonStyle(credential.disabledReason);
  const isThrottled = !credential.disabled && throttleRemaining > 0;

  // CardandListrowtotaluseofStatusborder stroke / gray out(Active · Overage · Cooldown · Disable)
  const stateClasses = [
    credential.isCurrent ? "ring-2 ring-primary/60 shadow-apple-lg" : "",
    !credential.disabled && isQuotaExceeded ? "ring-1 ring-amber-500/60" : "",
    disabledByQuota
      ? "ring-1 ring-amber-500/70 bg-amber-50/40 dark:bg-amber-500/[0.04]"
      : "",
    isThrottled
      ? "ring-1 ring-orange-500/60 bg-orange-50/40 dark:bg-orange-500/[0.04]"
      : "",
    credential.disabled && !disabledByQuota ? "opacity-70" : "",
  ]
    .filter(Boolean)
    .join(" ");

  // subscription / Status / authenticate / Groupetc.badge —— Cardheader andListrowtotaluse
  const badges = (
    <>
      {balance?.subscriptionTitle && (
        <SubscriptionBadge
          title={balance.subscriptionTitle}
          className="max-w-full"
        />
      )}
      {credential.isCurrent && <Badge variant="success">Active</Badge>}
      {/* DisableStatus:merge "Disabled" + localize to Chineseofreason,singleitems Badge more prominent */}
      {credential.disabled && reasonStyle && (
        <Badge variant={reasonStyle.variant}>Disabled · {reasonStyle.label}</Badge>
      )}
      {credential.disabled && !reasonStyle && (
        <Badge variant="destructive">Disabled</Badge>
      )}
      {/* stillEnablebut the limit is already reached:yellow"Over quota"badge */}
      {!credential.disabled && isQuotaExceeded && (
        <Badge variant="warning">Over quota</Badge>
      )}
      {isThrottled && (
        <Badge
          variant="warning"
          className="bg-orange-500/15 text-orange-700 dark:text-orange-300 border-orange-500/30"
          title="Account-level throttle cooling down (429 + suspicious activity). Scheduling resumes after it expires or is manually released"
        >
          <Clock className="mr-1 h-3 w-3" />
          Cooldown {formatThrottleCountdown(throttleRemaining)}
        </Badge>
      )}
      {credential.authMethod && <Badge variant="secondary">{authLabel}</Badge>}
      {/* Configmerge meta infoassingleitemsbadge,reduce line breaks:endpoint · ARN */}
      {(credential.endpoint || credential.hasProfileArn) && (
        <Badge
          variant="outline"
          className="max-w-full truncate"
          title={
            credential.hasProfileArn ? "endpoint / Configured Profile ARN" : "endpoint"
          }
        >
          {[credential.endpoint, credential.hasProfileArn ? "ARN" : null]
            .filter(Boolean)
            .join(" · ")}
        </Badge>
      )}
      {/* Accountbelongs toGroup */}
      {(credential.groups ?? []).map((g) => (
        <Badge key={g} variant="outline" title="Account group">
          {g}
        </Badge>
      ))}
      {/* Account source channel */}
      {credential.sourceChannel && (
        <Badge variant="outline" title="Account source channel">
          Source: {credential.sourceChannel}
        </Badge>
      )}
    </>
  );

  // “More actions”dropdown —— CardandListrowtotaluse
  const moreMenu = (
    // modal={false}:menunotmodal,avoid Radix in <html> apply on top overflow:hidden scroll lock.
    // the lockinmobile(especially iOS Safari)will overlap the background layer backdrop-blur / fixed position overlay,
    // causes the wholepagerender glitchorhorizontal shift——this is exactly the moveEndpointclick"More actions"afterpagesurfaceErrorofroot cause.
    <DropdownMenu modal={false}>
      <DropdownMenuTrigger asChild>
        <Button size="icon" variant="ghost" title="More actions">
          <MoreHorizontal className="h-4 w-4" />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end">
        <DropdownMenuItem
          onSelect={(e) => {
            e.preventDefault();
            handleReset();
          }}
          disabled={
            resetFailure.isPending ||
            (credential.failureCount === 0 &&
              credential.refreshFailureCount === 0)
          }
        >
          <RotateCcw />
          Reset failure count
        </DropdownMenuItem>
        <DropdownMenuItem
          onSelect={() => setShowModelsDialog(true)}
          disabled={credential.disabled}
          title={credential.disabled ? "Disabled credentials cannot be queried" : undefined}
        >
          <Boxes />
          View available models
        </DropdownMenuItem>
        {throttleRemaining > 0 && (
          <DropdownMenuItem
            onSelect={(e) => {
              e.preventDefault();
              handleClearThrottle();
            }}
            disabled={clearThrottle.isPending}
          >
            <Clock />
            Release throttle cooldown ({formatThrottleCountdown(throttleRemaining)})
          </DropdownMenuItem>
        )}
        {balance?.overageCapable === true &&
          (balance.overageEnabled ? (
            <DropdownMenuItem
              onSelect={(e) => {
                e.preventDefault();
                handleSetOverage(false);
              }}
              disabled={overageBusy}
            >
              <ZapOff />
              Disable overage
            </DropdownMenuItem>
          ) : (
            <DropdownMenuItem
              onSelect={(e) => {
                e.preventDefault();
                handleSetOverage(true);
              }}
              disabled={overageBusy}
            >
              <Zap className="text-emerald-500" />
              Enable overage
            </DropdownMenuItem>
          ))}
        {credential.authMethod !== "api_key" && <DropdownMenuSeparator />}
        {credential.authMethod !== "api_key" && (
          <DropdownMenuItem onSelect={() => setShowReloginDialog(true)}>
            <LogIn />
            Log in again
          </DropdownMenuItem>
        )}
        {credential.authMethod !== "api_key" && (
          <DropdownMenuItem onSelect={() => setShowUpdateTokenDialog(true)}>
            <RefreshCw />
            Reimport Token
          </DropdownMenuItem>
        )}
        <DropdownMenuSeparator />
        <DropdownMenuItem
          destructive
          onSelect={(e) => {
            e.preventDefault();
            setShowDeleteDialog(true);
          }}
        >
          <Trash2 />
          Delete credential
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );

  // compactListrow:inheritCardofAllActions(Enable/Disable · Priority · Failed/Success · Refresh · Edit · More · drag · select)
  const listView = (
    <div
      ref={setNodeRef}
      style={dragStyle}
      data-credential-id={credential.id}
      className={`group flex min-w-0 items-center gap-2 rounded-2xl border bg-card px-2 py-2 transition-all sm:gap-3 sm:px-3 ${
        isDragging
          ? "shadow-apple-lg opacity-80"
          : "hover:bg-accent/40 hover:shadow-apple-sm"
      } ${stateClasses}`}
    >
      {/* drag handle */}
      <Button
        ref={setActivatorNodeRef}
        size="icon"
        variant="ghost"
        data-no-rect-select
        className="h-8 w-8 shrink-0 cursor-grab touch-none active:cursor-grabbing"
        title="Drag to adjust priority"
        {...attributes}
        {...listeners}
      >
        <GripVertical className="h-4 w-4 text-muted-foreground" />
      </Button>

      {/* select box */}
      <label
        data-no-rect-select
        className="flex h-8 w-8 shrink-0 cursor-pointer items-center justify-center rounded-md transition-colors hover:bg-accent"
        onClick={(e) => e.stopPropagation()}
      >
        <Checkbox
          className="h-5 w-5 [&_svg]:h-4 [&_svg]:w-4"
          checked={selected}
          onCheckedChange={onToggleSelect}
        />
      </label>

      {/* identity + badge */}
      <div className="min-w-0 flex-1">
        <div className="truncate text-sm font-medium leading-5">
          {credential.email || `Credential #${credential.id}`}
        </div>
        <div className="mt-1 flex min-w-0 items-center gap-1 overflow-hidden [&>*]:shrink-0">
          {badges}
        </div>
      </div>

      {/* key metric(medium and large screens) */}
      <div className="hidden shrink-0 items-center gap-5 lg:flex">
        <div className="relative w-14 shrink-0 text-center">
          <div className="text-[10px] uppercase tracking-wider text-muted-foreground">
            Priority
          </div>
          {/* fixed height placeholder,avoidEditrow height jitters on state switch */}
          <div className="mt-0.5 flex h-[26px] items-center justify-center">
            {editingPriority ? (
              // Editbar(≈112px)wider than the column(56px)wider:absolute position out of flowStreaminglayout lifts up,
              // paired with the background and z-index,avoid being by adjacent"Failed"columninoverlays in draw order
              <div className="absolute left-1/2 top-1/2 z-30 inline-flex -translate-x-1/2 -translate-y-1/2 items-center gap-0.5 rounded-md border border-border/60 bg-card p-1 shadow-apple-sm">
                <Input
                  type="number"
                  value={priorityValue}
                  onChange={(e) => setPriorityValue(e.target.value)}
                  onKeyDown={(e) => {
                    if (e.key === "Enter") handlePriorityChange();
                    if (e.key === "Escape") {
                      setEditingPriority(false);
                      setPriorityValue(String(credential.priority));
                    }
                  }}
                  className="h-7 w-16 rounded-md text-sm"
                  min="0"
                  autoFocus
                />
                <Button
                  size="icon"
                  variant="ghost"
                  className="h-7 w-7"
                  onClick={handlePriorityChange}
                  disabled={setPriority.isPending}
                  title="Confirm"
                >
                  ✓
                </Button>
                <Button
                  size="icon"
                  variant="ghost"
                  className="h-7 w-7"
                  onClick={() => {
                    setEditingPriority(false);
                    setPriorityValue(String(credential.priority));
                  }}
                  title="Cancel"
                >
                  ✕
                </Button>
              </div>
            ) : (
              <button
                type="button"
                className="inline-flex items-center gap-1 rounded px-1.5 py-0.5 text-sm font-medium tabular-nums transition-colors hover:bg-accent hover:text-primary"
                onClick={() => setEditingPriority(true)}
                title="Click to edit priority"
              >
                {credential.priority}
                <Pencil className="h-3 w-3 opacity-70" />
              </button>
            )}
          </div>
        </div>

        <div className="w-20 text-center">
          <div className="text-[10px] uppercase tracking-wider text-muted-foreground">
            Failed
          </div>
          <button
            type="button"
            onClick={() => setShowFailuresDialog(true)}
            className="mt-0.5 inline-flex items-center gap-1 rounded px-1.5 py-0.5 text-sm font-medium tabular-nums transition-colors hover:bg-accent"
            title="Authentication failed / Account throttle / Other (quota·Transient·network, etc.). Click to view failure log details"
          >
            {failureStats ? (
              <span className="tabular-nums">
                <span className="text-destructive">{failureStats.auth}</span>
                <span className="text-muted-foreground/50">/</span>
                <span className="text-amber-600 dark:text-amber-400">
                  {failureStats.throttle}
                </span>
                <span className="text-muted-foreground/50">/</span>
                <span className="text-muted-foreground">
                  {failureStats.other}
                </span>
              </span>
            ) : (
              <span
                className={
                  credential.totalFailureCount > 0
                    ? "text-destructive"
                    : "text-muted-foreground"
                }
              >
                {credential.totalFailureCount}
              </span>
            )}
            <ScrollText className="h-3.5 w-3.5 opacity-70" />
          </button>
        </div>

        <div className="w-16 text-center">
          <div className="text-[10px] uppercase tracking-wider text-muted-foreground">
            Success
          </div>
          <button
            type="button"
            onClick={handleResetSuccess}
            className="mt-0.5 inline-flex items-center gap-1 rounded px-1.5 py-0.5 text-sm font-medium tabular-nums transition-colors hover:bg-accent hover:text-primary"
            title="Click to reset the success count"
          >
            {credential.successCount}
            <RotateCcw className="h-3 w-3 opacity-70" />
          </button>
        </div>
      </div>

      {/* Balance(large screen) */}
      <div className="hidden w-44 shrink-0 xl:block">
        {loadingBalance ? (
          <div className="flex items-center justify-center gap-1.5 text-xs text-muted-foreground">
            <Loader2 className="h-3.5 w-3.5 animate-spin" />
            Querying…
          </div>
        ) : balance ? (
          <div>
            <div className="flex items-baseline justify-between gap-2 text-xs tabular-nums">
              <span
                className={`font-semibold ${
                  balance.remaining < 0
                    ? "text-red-600 dark:text-red-400"
                    : balance.remaining === 0
                      ? "text-amber-600 dark:text-amber-400"
                      : "text-emerald-600 dark:text-emerald-400"
                }`}
              >
                {balance.remaining < 0
                  ? `-$${formatNumber(Math.abs(balance.remaining))}`
                  : `$${formatNumber(balance.remaining)}`}
              </span>
              <span className="text-muted-foreground">
                {balance.usagePercentage.toFixed(0)}%
              </span>
            </div>
            <Progress value={balance.usagePercentage} className="mt-1 h-1.5" />
          </div>
        ) : (
          <div className="text-center text-[11px] text-muted-foreground">
            Balance not queried
          </div>
        )}
      </div>

      {/* Last call(medium and large screens) */}
      <div className="hidden w-24 shrink-0 truncate text-right text-xs text-muted-foreground md:block">
        {formatLastUsed(credential.lastUsedAt)}
      </div>

      {/* Actionsregion */}
      <div className="flex shrink-0 items-center gap-0.5 sm:gap-1">
        <Button
          size="icon"
          variant="ghost"
          className="hidden h-9 w-9 sm:inline-flex"
          onClick={handleForceRefresh}
          disabled={
            forceRefresh.isPending ||
            credential.disabled ||
            credential.authMethod === "api_key"
          }
          title={
            credential.authMethod === "api_key"
              ? "API Key No refresh needed"
              : credential.disabled
                ? "Disabled"
                : "Force refresh Token"
          }
        >
          <RefreshCw
            className={`h-4 w-4 ${forceRefresh.isPending ? "animate-spin" : ""}`}
          />
        </Button>
        <Button
          size="icon"
          variant="ghost"
          className="hidden h-9 w-9 sm:inline-flex"
          onClick={onRefreshBalance}
          disabled={loadingBalance || credential.disabled}
          title={credential.disabled ? "Disabled" : "Refresh balance"}
        >
          {loadingBalance ? (
            <Loader2 className="h-4 w-4 animate-spin" />
          ) : (
            <Wallet className="h-4 w-4" />
          )}
        </Button>
        <Switch
          checked={!credential.disabled}
          onCheckedChange={handleToggleDisabled}
          disabled={setDisabled.isPending}
          title={credential.disabled ? "Enable" : "Disable"}
        />
        <Button
          size="icon"
          variant="ghost"
          className="h-9 w-9"
          onClick={() => setShowEditDialog(true)}
          title="Edit"
        >
          <Pencil className="h-4 w-4" />
        </Button>
        {moreMenu}
      </div>
    </div>
  );

  return (
    <>
      {view === "list" ? (
        listView
      ) : (
      <Card
        ref={setNodeRef}
        style={dragStyle}
        data-credential-id={credential.id}
        className={`group flex h-full min-w-0 flex-col ${
          isDragging
            ? "shadow-apple-lg opacity-80"
            : "hover:-translate-y-0.5 hover:shadow-apple-lg"
        } ${stateClasses}`}
      >
        <CardHeader className="p-4 pb-3 sm:p-5 sm:pb-3">
          <div className="flex min-w-0 items-start gap-2.5 sm:gap-3">
            <label
              data-no-rect-select
              className="mt-0.5 flex h-8 w-8 shrink-0 cursor-pointer items-center justify-center rounded-md transition-colors hover:bg-accent sm:h-7 sm:w-7"
              onClick={(e) => {
                // label + Checkbox dedupe double click events,avoid triggering twotimes onCheckedChange
                e.stopPropagation();
              }}
            >
              <Checkbox
                className="h-5 w-5 [&_svg]:h-4 [&_svg]:w-4"
                checked={selected}
                onCheckedChange={onToggleSelect}
              />
            </label>
            <div className="min-w-0 flex-1">
              <CardTitle className="truncate text-[15px] leading-5">
                {credential.email || `Credential #${credential.id}`}
              </CardTitle>
              <div className="mt-1.5 flex min-w-0 flex-wrap items-center gap-1 overflow-hidden">
                {badges}
              </div>
            </div>
            <Switch
              className="mt-0.5"
              checked={!credential.disabled}
              onCheckedChange={handleToggleDisabled}
              disabled={setDisabled.isPending}
              title={credential.disabled ? "Enable" : "Disable"}
            />
          </div>
        </CardHeader>

        <CardContent className="flex flex-1 flex-col space-y-3 px-4 pb-4 sm:space-y-4 sm:px-5 sm:pb-5">
          {/* info row */}
          <dl className="grid grid-cols-1 gap-2 text-[13px] min-[420px]:grid-cols-2 min-[420px]:gap-x-4">
            <div className="flex min-w-0 items-center justify-between gap-2">
              <dt className="shrink-0 text-muted-foreground">Priority</dt>
              <dd className="min-w-0">
                {editingPriority ? (
                  <div className="inline-flex max-w-full items-center gap-1">
                    <Input
                      type="number"
                      value={priorityValue}
                      onChange={(e) => setPriorityValue(e.target.value)}
                      className="w-16 h-7 rounded-md text-base sm:text-sm"
                      min="0"
                    />
                    <Button
                      size="icon"
                      variant="ghost"
                      className="h-7 w-7"
                      onClick={handlePriorityChange}
                      disabled={setPriority.isPending}
                    >
                      ✓
                    </Button>
                    <Button
                      size="icon"
                      variant="ghost"
                      className="h-7 w-7"
                      onClick={() => {
                        setEditingPriority(false);
                        setPriorityValue(String(credential.priority));
                      }}
                    >
                      ✕
                    </Button>
                  </div>
                ) : (
                  <button
                    type="button"
                    className="inline-flex cursor-pointer items-center gap-1 rounded px-1.5 py-0.5 font-medium tabular-nums transition-colors hover:bg-accent hover:text-primary"
                    onClick={() => setEditingPriority(true)}
                    title="Click to edit priority"
                  >
                    {credential.priority}
                    <Pencil className="h-3 w-3 opacity-70" />
                  </button>
                )}
              </dd>
            </div>
            <div className="flex min-w-0 items-center justify-between gap-2">
              <dt className="shrink-0 text-muted-foreground">Failure count</dt>
              <dd className="min-w-0">
                <button
                  type="button"
                  onClick={() => setShowFailuresDialog(true)}
                  className="inline-flex cursor-pointer items-center gap-1 rounded px-1.5 py-0.5 font-medium tabular-nums transition-colors hover:bg-accent"
                  title="Authentication failed / Account throttle / Other (quota·Transient·network, etc.). Click to view failure log details"
                >
                  {failureStats ? (
                    <span className="tabular-nums">
                      <span className="text-destructive">{failureStats.auth}</span>
                      <span className="text-muted-foreground/50">/</span>
                      <span className="text-amber-600 dark:text-amber-400">
                        {failureStats.throttle}
                      </span>
                      <span className="text-muted-foreground/50">/</span>
                      <span className="text-muted-foreground">{failureStats.other}</span>
                    </span>
                  ) : (
                    <span
                      className={
                        credential.totalFailureCount > 0
                          ? "text-destructive"
                          : "text-muted-foreground"
                      }
                    >
                      {credential.totalFailureCount}
                    </span>
                  )}
                  <ScrollText className="h-3.5 w-3.5 opacity-70" />
                </button>
              </dd>
            </div>
            <div className="flex min-w-0 items-center justify-between gap-2">
              <dt className="shrink-0 text-muted-foreground">Refresh failed</dt>
              <dd
                className={`tabular-nums font-medium ${credential.refreshFailureCount > 0 ? "text-destructive" : ""}`}
              >
                {credential.refreshFailureCount}
              </dd>
            </div>
            <div className="flex min-w-0 items-center justify-between gap-2">
              <dt className="shrink-0 text-muted-foreground">Success count</dt>
              <dd className="min-w-0">
                <button
                  type="button"
                  onClick={handleResetSuccess}
                  className="inline-flex cursor-pointer items-center gap-1 rounded px-1.5 py-0.5 font-medium tabular-nums transition-colors hover:bg-accent hover:text-primary"
                  title="Click to reset the success count"
                >
                  {credential.successCount}
                  <RotateCcw className="h-3 w-3 opacity-70" />
                </button>
              </dd>
            </div>
            <div className="flex min-w-0 items-center justify-between gap-2 border-t border-border/50 pt-2 min-[420px]:col-span-2">
              <dt className="shrink-0 text-muted-foreground">Last call</dt>
              <dd className="min-w-0 truncate text-right font-medium">
                {formatLastUsed(credential.lastUsedAt)}
              </dd>
            </div>
            {credential.maskedApiKey && (
              <div className="flex min-w-0 items-center justify-between gap-2 min-[420px]:col-span-2">
                <dt className="shrink-0 text-muted-foreground">API Key</dt>
                <dd className="min-w-0 truncate text-right font-mono text-xs">
                  {credential.maskedApiKey}
                </dd>
              </div>
            )}
            {credential.hasProxy && (
              <div className="flex min-w-0 items-center justify-between gap-2 min-[420px]:col-span-2">
                <dt className="shrink-0 text-muted-foreground">Proxy</dt>
                <dd className="min-w-0 truncate text-right font-mono text-xs">
                  {maskProxyUrl(credential.proxyUrl ?? "")}
                </dd>
              </div>
            )}
          </dl>

          {/* Balancepanel */}
          <div
            className={`flex min-h-[138px] flex-col rounded-xl border p-3 transition-colors sm:min-h-[150px] sm:p-4 ${
              isQuotaExceeded || disabledByQuota
                ? "border-amber-500/40 bg-amber-50/60 dark:bg-amber-500/[0.06]"
                : "border-border/60 bg-secondary/40"
            }`}
          >
            {loadingBalance ? (
              <div className="flex flex-1 items-center justify-center gap-2 text-sm text-muted-foreground">
                <Loader2 className="h-4 w-4 animate-spin" />
                Querying balance…
              </div>
            ) : balance ? (
              <div className="space-y-3">
                <div className="flex min-w-0 items-end justify-between gap-3">
                  <div className="min-w-0">
                    <div className="text-[11px] uppercase tracking-wider text-muted-foreground">
                      {balance.remaining < 0 ? "Overage" : "Balance"}
                    </div>
                    <div
                      className={`mt-0.5 text-xl font-semibold tabular-nums ${
                        balance.remaining < 0
                          ? "text-red-600 dark:text-red-400"
                          : balance.remaining === 0
                            ? "text-amber-600 dark:text-amber-400"
                            : "text-emerald-600 dark:text-emerald-400"
                      }`}
                    >
                      {balance.remaining < 0
                        ? `-$${formatNumber(Math.abs(balance.remaining))}`
                        : `$${formatNumber(balance.remaining)}`}
                    </div>
                  </div>
                  <div className="min-w-0 shrink-0 text-right">
                    <div className="text-[11px] uppercase tracking-wider text-muted-foreground">
                      Overage
                    </div>
                    <div className="mt-1 flex items-center justify-end">
                      <OverageStatusPill balance={balance} />
                    </div>
                  </div>
                </div>
                <div className="space-y-1.5">
                  <Progress value={balance.usagePercentage} />
                  <div className="grid grid-cols-3 gap-1 text-[11px] tabular-nums text-muted-foreground">
                    <span className="min-w-0 truncate">
                      Used ${formatNumber(balance.currentUsage)}
                    </span>
                    <span className="text-center">
                      {balance.usagePercentage.toFixed(1)}%
                    </span>
                    <span className="min-w-0 truncate text-right">
                      Quota ${formatNumber(balance.usageLimit)}
                    </span>
                  </div>
                </div>
                <div className="break-words border-t border-border/50 pt-2 text-[11px] text-muted-foreground">
                  Next reset:
                  <span className="font-medium text-foreground">
                    {formatResetDate(balance.nextResetAt)}
                  </span>
                </div>
              </div>
            ) : (
              <div className="flex flex-1 items-center justify-center text-center text-[13px] text-muted-foreground">
                Balance not queried. Click the top"Refresh the balance on the current page"to load.
              </div>
            )}
          </div>

          {/* Actionsregion */}
          <div className="mt-auto flex flex-col gap-2 border-t border-border/50 pt-3 min-[420px]:flex-row min-[420px]:items-center min-[420px]:justify-between">
            <div className="grid grid-cols-3 gap-1 min-[420px]:flex min-[420px]:items-center">
              <Button
                ref={setActivatorNodeRef}
                size="icon"
                variant="ghost"
                data-no-rect-select
                className="w-full cursor-grab touch-none active:cursor-grabbing min-[420px]:w-9"
                title="Drag to adjust priority"
                {...attributes}
                {...listeners}
              >
                <GripVertical className="h-4 w-4 text-muted-foreground" />
              </Button>
              <span className="mx-1 hidden h-5 w-px bg-border/70 min-[420px]:inline-block" />
              <Button
                size="sm"
                variant="ghost"
                className="w-full px-2 min-[420px]:w-auto min-[420px]:px-3"
                onClick={handleForceRefresh}
                disabled={
                  forceRefresh.isPending ||
                  credential.disabled ||
                  credential.authMethod === "api_key"
                }
                title={
                  credential.authMethod === "api_key"
                    ? "API Key No refresh needed"
                    : credential.disabled
                      ? "Disabled"
                      : "Force refresh Token"
                }
              >
                <RefreshCw
                  className={`h-3.5 w-3.5 ${forceRefresh.isPending ? "animate-spin" : ""}`}
                />
                <span className="hidden sm:inline">Refresh Token</span>
              </Button>
              <Button
                size="sm"
                variant="ghost"
                className="w-full px-2 min-[420px]:w-auto min-[420px]:px-3"
                onClick={onRefreshBalance}
                disabled={loadingBalance || credential.disabled}
                title={credential.disabled ? "Disabled" : "Refresh balance"}
              >
                <RefreshCw
                  className={`h-3.5 w-3.5 ${loadingBalance ? "animate-spin" : ""}`}
                />
                <span className="hidden sm:inline">Refresh balance</span>
              </Button>
            </div>

            <div className="grid grid-cols-[1fr_auto] gap-1 min-[420px]:flex min-[420px]:items-center">
              <Button
                size="sm"
                variant="outline"
                className="w-full min-[420px]:w-auto"
                onClick={() => setShowEditDialog(true)}
              >
                <Pencil className="h-3.5 w-3.5" />
                Edit
              </Button>
              {moreMenu}
            </div>
          </div>
        </CardContent>
      </Card>
      )}

      <Dialog open={showDeleteDialog} onOpenChange={setShowDeleteDialog}>
        <DialogContent className="sm:max-w-sm">
          <DialogHeader>
            <DialogTitle>Confirm delete credential</DialogTitle>
            <DialogDescription>
              Are you sure you want to delete the credential #{credential.id} ? This action cannot be undone.
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button
              variant="outline"
              onClick={() => setShowDeleteDialog(false)}
              disabled={deleteCredential.isPending}
            >
              Cancel
            </Button>
            <Button
              variant="destructive"
              onClick={handleDelete}
              disabled={deleteCredential.isPending}
            >
              Confirm delete
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <EditCredentialDialog
        open={showEditDialog}
        onOpenChange={setShowEditDialog}
        credential={credential}
      />
      <UpdateTokenDialog
        open={showUpdateTokenDialog}
        onOpenChange={setShowUpdateTokenDialog}
        credential={credential}
      />
      <ReloginDialog
        open={showReloginDialog}
        onOpenChange={setShowReloginDialog}
        credential={credential}
      />
      <CredentialFailuresDialog
        open={showFailuresDialog}
        onOpenChange={setShowFailuresDialog}
        credentialId={credential.id}
        email={credential.email}
      />
      <AvailableModelsDialog
        open={showModelsDialog}
        onOpenChange={setShowModelsDialog}
        credentialId={showModelsDialog ? credential.id : null}
      />
    </>
  );
}
