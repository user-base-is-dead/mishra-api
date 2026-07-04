import { useState } from 'react'
import { toast } from 'sonner'
import {
  ScrollText,
  RefreshCw,
  ChevronRight,
  ChevronLeft,
  ChevronDown,
  AlertTriangle,
  CheckCircle2,
  Unplug,
  Settings2,
} from 'lucide-react'
import { Card, CardContent } from '@/components/ui/card'
import { Button } from '@/components/ui/button'
import { Badge } from '@/components/ui/badge'
import { Input } from '@/components/ui/input'
import { Switch } from '@/components/ui/switch'
import {
  DropdownMenu,
  DropdownMenuTrigger,
  DropdownMenuContent,
  DropdownMenuLabel,
} from '@/components/ui/dropdown-menu'
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from '@/components/ui/tooltip'
import {
  Select as UiSelect,
  SelectTrigger as UiSelectTrigger,
  SelectValue as UiSelectValue,
  SelectContent as UiSelectContent,
  SelectItem as UiSelectItem,
} from '@/components/ui/select'
import { useTraces } from '@/hooks/use-traces'
import { useClientKeys } from '@/hooks/use-client-keys'
import { useGroupOptions } from '@/hooks/use-groups'
import {
  useLogGovernanceConfig,
  useSetLogGovernanceConfig,
} from '@/hooks/use-credentials'
import { extractErrorMessage } from '@/lib/utils'
import type { TraceAttempt, TraceQuery, TraceRecord } from '@/types/api'

/** Failedcategory → Chinese label + Badge color */
function outcomeStyle(outcome: string): {
  label: string
  variant: 'default' | 'secondary' | 'destructive' | 'outline' | 'success' | 'warning'
} {
  switch (outcome) {
    case 'success':
      return { label: 'Success', variant: 'success' }
    case 'quota_exhausted':
      return { label: 'Quota exhausted', variant: 'warning' }
    case 'account_throttled':
      return { label: 'Account throttle', variant: 'warning' }
    case 'auth_failed':
      return { label: 'Authentication failed', variant: 'destructive' }
    case 'transient':
      return { label: 'Transient error', variant: 'outline' }
    case 'network_error':
      return { label: 'Network error', variant: 'destructive' }
    case 'bad_request':
      return { label: 'Request error', variant: 'destructive' }
    case 'stream_interrupted':
      return { label: 'Stream interrupted', variant: 'warning' }
    default:
      return { label: outcome || 'Unknown', variant: 'secondary' }
  }
}

/** finalStatus → badge */
function StatusBadge({ status }: { status: string }) {
  if (status === 'success')
    return (
      <Badge variant="success">
        <CheckCircle2 className="mr-1 h-3 w-3" />
        Success
      </Badge>
    )
  if (status === 'interrupted')
    return (
      <Badge variant="warning">
        <Unplug className="mr-1 h-3 w-3" />
        Interrupted
      </Badge>
    )
  return (
    <Badge variant="destructive">
      <AlertTriangle className="mr-1 h-3 w-3" />
      Failed
    </Badge>
  )
}

function formatTime(ts: string): string {
  const d = new Date(ts)
  if (isNaN(d.getTime())) return ts
  return d.toLocaleString('zh-CN', { hour12: false })
}

function formatDuration(ms: number): string {
  if (ms < 1000) return `${ms}ms`
  return `${(ms / 1000).toFixed(2)}s`
}

function formatTokens(n: number): string {
  if (n >= 1_000_000) return (n / 1_000_000).toFixed(2) + 'M'
  if (n >= 1_000) return (n / 1_000).toFixed(1) + 'K'
  return String(n)
}

/** thousands separatoroffull value(Used forDetailsfloating box) */
function formatTokenFull(n: number): string {
  return n.toLocaleString('en-US')
}

function credLabel(id: number, email?: string | null): string {
  if (id === 0) return '—'
  return email ? email : `#${id}`
}

function keyLabel(keyId: number, keyName?: string | null): string {
  if (keyName) return keyName
  return `#${keyId}`
}

const STATUS_OPTIONS = [
  { value: '', label: 'All statuses' },
  { value: 'success', label: 'Success' },
  { value: 'error', label: 'Failed' },
  { value: 'interrupted', label: 'Interrupted' },
]

const ERROR_TYPE_OPTIONS = [
  { value: '', label: 'All error types' },
  { value: 'quota_exhausted', label: 'Quota exhausted' },
  { value: 'account_throttled', label: 'Account throttle' },
  { value: 'auth_failed', label: 'Authentication failed' },
  { value: 'transient', label: 'Transient error' },
  { value: 'network_error', label: 'Network error' },
  { value: 'bad_request', label: 'Request error' },
  { value: 'stream_interrupted', label: 'Stream interrupted' },
  { value: 'unknown', label: 'Unknown' },
]

/** singlejumpDetailsrow */
function AttemptRow({ a }: { a: TraceAttempt }) {
  const style = outcomeStyle(a.outcome)
  return (
    <div className="rounded-lg border border-border/50 bg-secondary/30 p-3">
      <div className="flex flex-wrap items-center gap-2 text-[13px]">
        <span className="font-mono text-muted-foreground">#{a.attempt}</span>
        <Badge variant={style.variant}>{style.label}</Badge>
        <span className="text-muted-foreground">Credential</span>
        <span className="font-medium">{credLabel(a.credentialId, a.email)}</span>
        {a.endpoint && <Badge variant="outline">{a.endpoint}</Badge>}
        <span className="text-muted-foreground">HTTP</span>
        <span className="font-mono">{a.httpStatus ?? '—'}</span>
        <span className="ml-auto font-mono text-muted-foreground">
          {formatDuration(a.durationMs)}
        </span>
      </div>
      {a.errorSnippet && (
        <pre className="mt-2 max-h-40 overflow-auto whitespace-pre-wrap break-all rounded-md bg-background/60 p-2 font-mono text-[11px] text-muted-foreground">
          {a.errorSnippet}
        </pre>
      )}
    </div>
  )
}

/** expandableoftrace row */
/** Token Usagecell:compact displaytotalamount,hover Showline itemDetails */
function TokenCell({ rec }: { rec: TraceRecord }) {
  const input = rec.inputTokens ?? 0
  const output = rec.outputTokens ?? 0
  const cacheCreation = rec.cacheCreationTokens ?? 0
  const cacheRead = rec.cacheReadTokens ?? 0
  const total = rec.totalTokens ?? input + output + cacheCreation + cacheRead
  // all 0(earlyFailed,did not reachUpstream)at that time notShowDetails,placeholder only
  if (total === 0) {
    return <span className="text-muted-foreground">—</span>
  }
  const rows: Array<[string, number]> = [
    ['Input Token', input],
    ['Output Token', output],
  ]
  if (cacheCreation > 0) rows.push(['Cache create Token', cacheCreation])
  if (cacheRead > 0) rows.push(['Cache read Token', cacheRead])
  return (
    <TooltipProvider delayDuration={150}>
      <Tooltip>
        <TooltipTrigger asChild>
          <span className="inline-flex items-center gap-1 font-mono tabular-nums cursor-default border-b border-dotted border-muted-foreground/40">
            <span className="text-emerald-600 dark:text-emerald-400">
              ↓{formatTokens(input + cacheCreation + cacheRead)}
            </span>
            <span className="text-violet-600 dark:text-violet-400">
              ↑{formatTokens(output)}
            </span>
          </span>
        </TooltipTrigger>
        <TooltipContent className="p-0">
          <div className="min-w-[180px] px-3 py-2">
            <div className="mb-1.5 text-[13px] font-semibold">Token Details</div>
            <div className="space-y-1 text-[12px]">
              {rows.map(([label, val]) => (
                <div key={label} className="flex items-center justify-between gap-6">
                  <span className="text-muted-foreground">{label}</span>
                  <span className="font-mono tabular-nums">{formatTokenFull(val)}</span>
                </div>
              ))}
              <div className="mt-1 flex items-center justify-between gap-6 border-t border-border/50 pt-1">
                <span className="font-medium">total Token</span>
                <span className="font-mono font-semibold tabular-nums">
                  {formatTokenFull(total)}
                </span>
              </div>
            </div>
          </div>
        </TooltipContent>
      </Tooltip>
    </TooltipProvider>
  )
}

function TraceRow({ rec }: { rec: TraceRecord }) {
  const [open, setOpen] = useState(false)
  const errStyle = rec.errorType ? outcomeStyle(rec.errorType) : null
  return (
    <>
      <tr
        className="cursor-pointer whitespace-nowrap border-b border-border/40 hover:bg-accent/40"
        onClick={() => setOpen((v) => !v)}
      >
        <td className="py-2.5 pl-3 pr-2">
          {open ? (
            <ChevronDown className="h-4 w-4 text-muted-foreground" />
          ) : (
            <ChevronRight className="h-4 w-4 text-muted-foreground" />
          )}
        </td>
        <td className="py-2.5 pr-3 text-[13px] tabular-nums text-muted-foreground whitespace-nowrap">
          {formatTime(rec.ts)}
        </td>
        <td className="py-2.5 pr-3 text-[13px]">
          <span className="inline-block max-w-[220px] truncate align-middle">{rec.model}</span>
          {rec.isStream && <Badge variant="outline" className="ml-1.5">Streaming</Badge>}
        </td>
        <td className="py-2.5 pr-3 text-[13px]">
          <Badge variant="outline">{keyLabel(rec.keyId, rec.keyName)}</Badge>
        </td>
        <td className="py-2.5 pr-3">
          <StatusBadge status={rec.finalStatus} />
        </td>
        <TraceCredentialCell rec={rec} />
        <td className="py-2.5 pr-3 text-[12px] tabular-nums">
          <TokenCell rec={rec} />
        </td>
        <td className="py-2.5 pr-3 text-[13px] tabular-nums">
          {rec.credits != null && rec.credits > 0 ? rec.credits.toFixed(4) : '—'}
        </td>
        <td className="py-2.5 pr-3 text-[13px] tabular-nums text-muted-foreground">
          {rec.firstTokenMs != null ? formatDuration(rec.firstTokenMs) : '—'}
        </td>
        <td className="py-2.5 pr-3">
          {errStyle ? <Badge variant={errStyle.variant}>{errStyle.label}</Badge> : '—'}
        </td>
        <td className="py-2.5 pr-3 text-[13px] tabular-nums">
          {Math.max(0, rec.totalAttempts - 1)}
        </td>
        <td className="py-2.5 pr-3 text-[13px] tabular-nums text-muted-foreground">
          {formatDuration(rec.durationMs)}
        </td>
      </tr>
      {open && <ExpandedTraceRow rec={rec} />}
    </>
  )
}

function TraceCredentialCell({ rec }: { rec: TraceRecord }) {
  return (
    <td className="py-2.5 pr-3 text-[13px]">
      <span className="inline-block max-w-[220px] truncate align-middle">
        {credLabel(rec.finalCredentialId, rec.finalEmail)}
      </span>
    </td>
  )
}

function ExpandedTraceRow({ rec }: { rec: TraceRecord }) {
  return (
    <tr className="border-b border-border/40 bg-secondary/20">
      <td colSpan={12} className="px-3 py-3">
        <ExpandedDetail rec={rec} />
      </td>
    </tr>
  )
}

/** after expandingoftrace detail:Errorsummary + eachjumpTimeline */
function ExpandedDetail({ rec }: { rec: TraceRecord }) {
  return (
    <div className="space-y-3">
      {rec.errorMessage && (
        <div className="rounded-lg border border-destructive/30 bg-destructive/5 p-3 text-[13px] text-destructive">
          {rec.errorMessage}
        </div>
      )}
      {rec.interruptedAfterBytes != null && (
        <div className="text-[12px] text-muted-foreground">
          sent before interruption {rec.interruptedAfterBytes} bytes
        </div>
      )}
      <div className="text-[12px] font-medium text-muted-foreground">
        Attempt trace ({rec.attempts.length} times
        {rec.attempts.length > 1 ? `, including ${rec.attempts.length - 1} retries` : ", not retried"})
      </div>
      <div className="space-y-2">
        {rec.attempts.length === 0 ? (
          <div className="text-[13px] text-muted-foreground">No attempt records (the request did not reach the upstream)</div>
        ) : (
          rec.attempts.map((a) => <AttemptRow key={a.attempt} a={a} />)
        )}
      </div>
    </div>
  )
}

/** dropdown filter */
function Select({
  value,
  onChange,
  options,
}: {
  value: string
  onChange: (v: string) => void
  options: { value: string; label: string }[]
}) {
  // radix Select empty not allowedstring value,use a sentinel "__all__" represents [empty/All],transparent to the outside.
  const SENTINEL = '__all__'
  return (
    <UiSelect
      value={value === '' ? SENTINEL : value}
      onValueChange={(v) => onChange(v === SENTINEL ? '' : v)}
    >
      <UiSelectTrigger className="h-8 w-auto min-w-[120px]">
        <UiSelectValue />
      </UiSelectTrigger>
      <UiSelectContent>
        {options.map((o) => (
          <UiSelectItem key={o.value} value={o.value === '' ? SENTINEL : o.value}>
            {o.label}
          </UiSelectItem>
        ))}
      </UiSelectContent>
    </UiSelect>
  )
}

/** LogsGovernance settingsdropdown:trace Enabletoggle + trace keepDays + usage keepDays */
function GovernanceButton() {
  const [open, setOpen] = useState(false)
  const { data: cfg, isLoading } = useLogGovernanceConfig()
  const { mutate, isPending } = useSetLogGovernanceConfig()
  const [traceDays, setTraceDays] = useState('')
  const [usageDays, setUsageDays] = useState('')

  const enabled = cfg?.traceEnabled ?? true

  const save = (patch: Record<string, unknown>, ok: string) => {
    mutate(patch, {
      onSuccess: () => toast.success(ok),
      onError: (err) => toast.error('Save failed:' + extractErrorMessage(err)),
    })
  }

  const submitDays = (
    e: React.FormEvent,
    field: 'traceRetentionDays' | 'usageLogRetentionDays',
    raw: string,
    reset: () => void,
  ) => {
    e.preventDefault()
    const n = parseInt(raw, 10)
    if (isNaN(n) || n < 1 || n > 365) {
      toast.error('Retention days must be within 1..=365')
      return
    }
    save({ [field]: n }, 'Retention days updated')
    reset()
  }

  return (
    <DropdownMenu open={open} onOpenChange={setOpen}>
      <DropdownMenuTrigger asChild>
        <Button size="sm" variant="outline">
          <Settings2 className="h-3.5 w-3.5" />
          Governance settings
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="w-72">
        <DropdownMenuLabel>Request trace</DropdownMenuLabel>
        <div className="px-2 pb-2">
          <div className="flex items-center justify-between gap-2 rounded-md bg-secondary/40 px-2.5 py-2">
            <div className="text-xs">
              <div className="font-medium text-foreground">
                {enabled ? 'Enabled' : 'Disabled'}
              </div>
              <div className="leading-snug text-muted-foreground">
                {enabled
                  ? 'Record the full retry trace of each request to traces.db'
                  : 'No new traces are written (historical records remain queryable)'}
              </div>
            </div>
            <Switch
              checked={enabled}
              disabled={isLoading || isPending}
              onCheckedChange={(v) =>
                save({ traceEnabled: v }, v ? 'Request trace enabled' : 'Request trace disabled')
              }
            />
          </div>
        </div>
        <DropdownMenuLabel className="pt-1">
          trace Retention days (currently {cfg?.traceRetentionDays ?? '—'})
        </DropdownMenuLabel>
        <form
          onSubmit={(e) => submitDays(e, 'traceRetentionDays', traceDays, () => setTraceDays(''))}
          className="flex items-center gap-1.5 px-2 pb-2"
        >
          <Input
            type="number"
            min={1}
            max={365}
            placeholder="Days"
            value={traceDays}
            onChange={(e) => setTraceDays(e.target.value)}
            disabled={isPending}
            className="h-7 text-xs"
          />
          <Button type="submit" size="sm" variant="outline" className="h-7 text-xs" disabled={isPending || !traceDays.trim()}>
            Save
          </Button>
        </form>
        <DropdownMenuLabel className="pt-1">
          usage Log retention days (currently {cfg?.usageLogRetentionDays ?? '—'})
        </DropdownMenuLabel>
        <form
          onSubmit={(e) => submitDays(e, 'usageLogRetentionDays', usageDays, () => setUsageDays(''))}
          className="flex items-center gap-1.5 px-2 pb-2"
        >
          <Input
            type="number"
            min={1}
            max={365}
            placeholder="Days"
            value={usageDays}
            onChange={(e) => setUsageDays(e.target.value)}
            disabled={isPending}
            className="h-7 text-xs"
          />
          <Button type="submit" size="sm" variant="outline" className="h-7 text-xs" disabled={isPending || !usageDays.trim()}>
            Save
          </Button>
        </form>
      </DropdownMenuContent>
    </DropdownMenu>
  )
}


const PAGE_SIZE = 50

export function TraceLogPage() {
  const [status, setStatus] = useState('')
  const [errorType, setErrorType] = useState('')
  const [keyId, setKeyId] = useState('')
  const [group, setGroup] = useState('')
  const [onlyFailed, setOnlyFailed] = useState(false)
  const [page, setPage] = useState(0)

  const { data: keysData } = useClientKeys()
  const keyOptions = [
    { value: '', label: 'All Key' },
    ...(keysData?.keys ?? []).map((k) => ({ value: String(k.id), label: k.name })),
  ]

  const groupOptions = useGroupOptions()
  const groupSelectOptions = [
    { value: '', label: 'All groups' },
    ...groupOptions.map((g) => ({ value: g, label: g })),
  ]

  // filteritemsreturn when the file changesNo.onepage
  const resetTo = <T,>(setter: (v: T) => void) => (v: T) => {
    setter(v)
    setPage(0)
  }

  const query: TraceQuery = {
    status: status || undefined,
    errorType: errorType || undefined,
    keyId: keyId ? Number(keyId) : undefined,
    group: group || undefined,
    onlyFailed: onlyFailed || undefined,
    limit: PAGE_SIZE,
    offset: page * PAGE_SIZE,
  }
  const { data, isLoading, isFetching, refetch } = useTraces(query)
  const records = data?.records ?? []
  const total = data?.total ?? 0
  const totalPages = Math.max(1, Math.ceil(total / PAGE_SIZE))

  return (
    <div className="space-y-5">
      {/* filter bar */}
      <div className="flex flex-wrap items-center gap-3">
        <div className="flex items-center gap-2">
          <ScrollText className="h-5 w-5 text-muted-foreground" />
          <h2 className="text-lg font-semibold tracking-tight">Request logs</h2>
          {total > 0 && <Badge variant="secondary">{total}</Badge>}
        </div>
        <div className="ml-auto flex flex-wrap items-center gap-2">
          <Select value={keyId} onChange={resetTo(setKeyId)} options={keyOptions} />
          <Select value={group} onChange={resetTo(setGroup)} options={groupSelectOptions} />
          <Select value={status} onChange={resetTo(setStatus)} options={STATUS_OPTIONS} />
          <Select
            value={errorType}
            onChange={resetTo(setErrorType)}
            options={ERROR_TYPE_OPTIONS}
          />
          <Button
            size="sm"
            variant={onlyFailed ? 'default' : 'outline'}
            onClick={() => {
              setOnlyFailed((v) => !v)
              setPage(0)
            }}
          >
            Show failures only
          </Button>
          <GovernanceButton />
          <Button size="sm" variant="outline" onClick={() => refetch()} disabled={isFetching}>
            <RefreshCw className={`h-3.5 w-3.5 ${isFetching ? 'animate-spin' : ''}`} />
            Refresh
          </Button>
        </div>
      </div>

      <Card>
        <CardContent className="p-0">
          {isLoading ? (
            <div className="p-6 text-sm text-muted-foreground">Loading…</div>
          ) : records.length === 0 ? (
            <div className="p-6 text-sm text-muted-foreground">
              No records yet. Make a few /v1/messages requests, then you can see the trace.
            </div>
          ) : (
            <div className="overflow-x-auto">
              <table className="w-full min-w-[1080px] text-left">
                <thead>
                  <tr className="whitespace-nowrap border-b border-border/60 text-[12px] uppercase tracking-wider text-muted-foreground">
                    <th className="py-2 pl-3 pr-2 font-medium"></th>
                    <th className="py-2 pr-3 font-medium">Time</th>
                    <th className="py-2 pr-3 font-medium">Model</th>
                    <th className="py-2 pr-3 font-medium">Entry Key</th>
                    <th className="py-2 pr-3 font-medium">Status</th>
                    <th className="py-2 pr-3 font-medium">Final credential</th>
                    <th className="py-2 pr-3 font-medium">Token</th>
                    <th className="py-2 pr-3 font-medium">Cost</th>
                    <th className="py-2 pr-3 font-medium">firstToken</th>
                    <th className="py-2 pr-3 font-medium">Error type</th>
                    <th className="py-2 pr-3 font-medium">Retry</th>
                    <th className="py-2 pr-3 font-medium">Duration</th>
                  </tr>
                </thead>
                <tbody>
                  {records.map((rec) => (
                    <TraceRow key={rec.traceId} rec={rec} />
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </CardContent>
      </Card>

      {total > PAGE_SIZE && (
        <div className="flex items-center justify-center gap-2">
          <Button
            variant="outline"
            size="sm"
            onClick={() => setPage((p) => Math.max(0, p - 1))}
            disabled={page === 0 || isFetching}
          >
            <ChevronLeft className="h-3.5 w-3.5" />
            Previous page
          </Button>
          <div className="px-3 text-sm tabular-nums text-muted-foreground">
            No. <span className="font-medium text-foreground">{page + 1}</span> /{' '}
            {totalPages} page
            <span className="mx-1.5 text-muted-foreground/50">·</span>total {total} items
          </div>
          <Button
            variant="outline"
            size="sm"
            onClick={() => setPage((p) => Math.min(totalPages - 1, p + 1))}
            disabled={page >= totalPages - 1 || isFetching}
          >
            Next page
            <ChevronRight className="h-3.5 w-3.5" />
          </Button>
        </div>
      )}
    </div>
  )
}




