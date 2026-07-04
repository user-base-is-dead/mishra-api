import { useEffect, useState } from 'react'
import {
  ChevronDown,
  CheckCircle2,
  Download,
  ExternalLink,
  Info,
  KeyRound,
  RefreshCw,
  RotateCcw,
  Save,
  ShieldCheck,
  Sparkles,
  UploadCloud,
  XCircle,
} from 'lucide-react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { toast } from 'sonner'
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Badge } from '@/components/ui/badge'
import { Switch } from '@/components/ui/switch'
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from '@/components/ui/tooltip'
import {
  applyImageUpdate,
  checkGitHubRateLimit,
  checkSystemUpdate,
  getUpdateConfig,
  pullUpdateImage,
  rollbackImageUpdate,
  setUpdateConfig,
} from '@/api/credentials'
import { extractErrorMessage } from '@/lib/utils'
import type { GitHubRateLimitInfo } from '@/types/api'
import { Markdown } from '@/components/markdown'

interface ImageUpdateDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
}

/** put RFC3339 Timeconvert to readable local timezonestring.Parse failedkeep as is at that timeBack. */
function formatDateTime(value: string): string {
  if (!value) return '—'
  const t = Date.parse(value)
  if (Number.isNaN(t)) return value
  return new Date(t).toLocaleString()
}

export function ImageUpdateDialog({ open, onOpenChange }: ImageUpdateDialogProps) {
  const queryClient = useQueryClient()
  const [autoApplyTime, setAutoApplyTime] = useState('03:00')
  const [lastOutput, setLastOutput] = useState('')
  const [tipsOpen, setTipsOpen] = useState(false)
  const [githubToken, setGithubToken] = useState('')

  const { data, isLoading } = useQuery({
    queryKey: ['update-config'],
    queryFn: getUpdateConfig,
    enabled: open,
  })

  // reuse when the dialog opens dashboard already startedofRequest;backend 30 minutescache to avoidDuplicatehit GitHub.
  const { data: updateCheck, isFetching: checkingUpdate } = useQuery({
    queryKey: ['system-update-check'],
    queryFn: () => checkSystemUpdate(false),
    enabled: open,
    staleTime: 5 * 60 * 1000,
  })

  // Rate limitStatus:run once automatically when the dialog openstimes (useSaved token),follow update-config refetch on change
  const { data: rateLimit, isFetching: checkingRate } = useQuery({
    queryKey: ['github-rate-limit', data?.githubTokenSet],
    queryFn: () => checkGitHubRateLimit(),
    enabled: open && data !== undefined,
    staleTime: 60 * 1000,
    retry: 0,
  })

  const refreshUpdateCheck = useMutation({
    mutationFn: () => checkSystemUpdate(true),
    onSuccess: (info) => {
      queryClient.setQueryData(['system-update-check'], info)
      if (info.warning) {
        toast.error(info.warning)
      } else if (info.hasUpdate) {
        toast.success(`New version found v${info.latestVersion}`)
      } else {
        toast.success('Already on the latest version')
      }
    },
    onError: (err) => toast.error(`Check for updates failed: ${extractErrorMessage(err)}`),
  })

  const autoApplyMutation = useMutation({
    mutationFn: (autoApply: boolean) => setUpdateConfig({ autoApply }),
    onMutate: async (autoApply) => {
      // do an optimistic update first,toggle switchofvisual feedback takes effect instantly
      const prev = queryClient.getQueryData<typeof data>(['update-config'])
      if (prev) {
        queryClient.setQueryData(['update-config'], { ...prev, autoApply })
      }
      return { prev }
    },
    onSuccess: (res) => {
      queryClient.setQueryData(['update-config'], res)
      toast.success(res.autoApply ? 'Auto-update enabled' : 'Auto-update disabled')
    },
    onError: (err, _variables, ctx) => {
      if (ctx?.prev) {
        queryClient.setQueryData(['update-config'], ctx.prev)
      }
      toast.error(`Switch failed: ${extractErrorMessage(err)}`)
    },
  })

  const autoApplyTimeMutation = useMutation({
    mutationFn: (autoApplyTime: string) => setUpdateConfig({ autoApplyTime }),
    onSuccess: (res) => {
      queryClient.setQueryData(['update-config'], res)
      toast.success(`Auto-update time set to ${res.autoApplyTime}`)
    },
    onError: (err) => toast.error(`Save time failed: ${extractErrorMessage(err)}`),
  })

  useEffect(() => {
    if (!data) return
    setAutoApplyTime(data.autoApplyTime || '03:00')
    // when the dialog opensput token Inputbox cleared:the backend does not echo plaintext,Inputboxasempty means"keep the original value"
    setGithubToken('')
  }, [data])

  const githubTokenMutation = useMutation({
    mutationFn: (token: string) => setUpdateConfig({ githubToken: token }),
    onSuccess: (res) => {
      queryClient.setQueryData(['update-config'], res)
      // SaveSuccessforce a recheck right after,let the user see the new token whether it can unlockRate limit
      queryClient.invalidateQueries({ queryKey: ['system-update-check'] })
      queryClient.invalidateQueries({ queryKey: ['github-rate-limit'] })
      setGithubToken('')
      toast.success(res.githubTokenSet ? 'GitHub Token Saved' : 'GitHub Token Cleared')
    },
    onError: (err) => toast.error(`Save failed: ${extractErrorMessage(err)}`),
  })

  // "Validate"button:useInputboxof token call onetimes /rate_limit,notSaveto config
  const verifyTokenMutation = useMutation({
    mutationFn: (token: string) => checkGitHubRateLimit(token),
    onSuccess: (info) => {
      if (info.valid) {
        toast.success(
          info.login
            ? `Token Valid, account ${info.login}, remaining ${info.remaining}/${info.limit}`
            : `Token Valid, remaining ${info.remaining}/${info.limit}`,
        )
      } else {
        toast.error(info.warning || 'Token Validation failed')
      }
    },
    onError: (err) => toast.error(`Validation failed: ${extractErrorMessage(err)}`),
  })

  const pullMutation = useMutation({
    mutationFn: pullUpdateImage,
    onSuccess: (res) => {
      setLastOutput(res.output || res.message)
      toast.success(res.message)
    },
    onError: (err) => toast.error(`Fetch failed: ${extractErrorMessage(err)}`),
  })

  const applyMutation = useMutation({
    mutationFn: applyImageUpdate,
    onSuccess: (res) => {
      setLastOutput(res.output || res.message)
      toast.success(res.message)
      queryClient.invalidateQueries({ queryKey: ['update-config'] })
    },
    onError: (err) => toast.error(`Update failed: ${extractErrorMessage(err)}`),
  })

  const rollbackMutation = useMutation({
    mutationFn: rollbackImageUpdate,
    onSuccess: (res) => {
      setLastOutput(res.output || res.message)
      toast.success(res.message)
      queryClient.invalidateQueries({ queryKey: ['update-config'] })
    },
    onError: (err) => toast.error(`Rollback failed: ${extractErrorMessage(err)}`),
  })

  const busy =
    isLoading ||
    pullMutation.isPending ||
    applyMutation.isPending ||
    rollbackMutation.isPending ||
    autoApplyMutation.isPending ||
    autoApplyTimeMutation.isPending ||
    githubTokenMutation.isPending ||
    verifyTokenMutation.isPending

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        aria-describedby={undefined}
        className="sm:max-w-2xl max-h-[85vh] overflow-y-auto"
        onOpenAutoFocus={(e) => {
          // prevent Radix Defaultputfocus lost toNo.oneitemsfocusable child element(info button),
          // otherwise Tooltip ofthe controlled switch is onFocus trigger immediately.
          e.preventDefault()
        }}
      >
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <UploadCloud className="h-4 w-4" />
            Online update
            <TooltipProvider delayDuration={0} disableHoverableContent={false}>
              <Tooltip open={tipsOpen} onOpenChange={setTipsOpen}>
                <TooltipTrigger asChild>
                  <button
                    type="button"
                    aria-label="Online update prerequisites"
                    onClick={() => setTipsOpen((v) => !v)}
                    onMouseEnter={() => setTipsOpen(true)}
                    onMouseLeave={() => setTipsOpen(false)}
                    className="inline-flex h-5 w-5 items-center justify-center rounded text-muted-foreground transition-colors hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/40"
                  >
                    <Info className="h-3.5 w-3.5" />
                  </button>
                </TooltipTrigger>
                <TooltipContent
                  side="bottom"
                  align="start"
                  sideOffset={6}
                  collisionPadding={12}
                  onMouseEnter={() => setTipsOpen(true)}
                  onMouseLeave={() => setTipsOpen(false)}
                >
                  <div className="mb-1 font-medium">Online update mechanism</div>
                  <ul className="list-disc space-y-1 pl-4">
                    <li>from GitHub Releases Download the new version binary and verify it SHA256</li>
                    <li>Atomically replace the current <code className="font-mono">kiro-rs</code>, the old version is backed up to <code className="font-mono">.backup</code></li>
                    <li>After the process exits, the container restart policy takes over the restart</li>
                  </ul>
                </TooltipContent>
              </Tooltip>
            </TooltipProvider>
          </DialogTitle>
        </DialogHeader>

        <div className="space-y-5 py-2">
          <section>
            <div className="flex items-start justify-between gap-2">
              <div className="flex items-center gap-2 text-sm font-medium text-foreground">
                <Sparkles className="h-4 w-4" />
                Version info
              </div>
              <Button
                type="button"
                size="sm"
                variant="outline"
                disabled={busy || refreshUpdateCheck.isPending}
                onClick={() => refreshUpdateCheck.mutate()}
              >
                {refreshUpdateCheck.isPending || checkingUpdate ? (
                  <RefreshCw className="h-3.5 w-3.5 animate-spin" />
                ) : (
                  <RefreshCw className="h-3.5 w-3.5" />
                )}
                <span className="ml-1.5">Check now</span>
              </Button>
            </div>

            <dl className="mt-3 grid grid-cols-1 gap-x-6 gap-y-1.5 text-xs sm:grid-cols-2">
              <div className="flex items-baseline gap-2">
                <dt className="w-20 shrink-0 text-muted-foreground">Current version</dt>
                <dd className="font-mono">
                  {updateCheck?.currentVersion
                    ? `v${updateCheck.currentVersion}`
                    : 'Loading…'}
                </dd>
              </div>
              <div className="flex items-baseline gap-2">
                <dt className="w-20 shrink-0 text-muted-foreground">Latest version</dt>
                <dd className="font-mono">
                  {updateCheck?.latestVersion
                    ? `v${updateCheck.latestVersion}`
                    : updateCheck
                      ? 'Unknown'
                      : 'Loading…'}
                  {updateCheck?.hasUpdate && (
                    <Badge variant="success" className="ml-2 align-middle">
                      Update available
                    </Badge>
                  )}
                </dd>
              </div>
              <div className="flex items-baseline gap-2">
                <dt className="w-20 shrink-0 text-muted-foreground">Build type</dt>
                <dd className="font-mono">
                  {updateCheck?.buildType || 'Loading…'}
                </dd>
              </div>
              <div className="flex items-baseline gap-2">
                <dt className="w-20 shrink-0 text-muted-foreground">Release time</dt>
                <dd className="font-mono">
                  {updateCheck?.publishedAt
                    ? formatDateTime(updateCheck.publishedAt)
                    : '—'}
                </dd>
              </div>
            </dl>

            {updateCheck?.releaseNotes && (
              <ReleaseNotesPanel
                version={updateCheck.latestVersion}
                title={updateCheck.releaseName}
                notes={updateCheck.releaseNotes}
                href={updateCheck.releaseUrl}
              />
            )}

            {!updateCheck?.releaseNotes && updateCheck?.releaseUrl && (
              <div className="mt-2 text-xs">
                <a
                  href={updateCheck.releaseUrl}
                  target="_blank"
                  rel="noreferrer"
                  className="underline hover:no-underline"
                >
                  View Release Notes
                </a>
              </div>
            )}

            {updateCheck?.warning && (
              <div className="mt-2 text-xs text-destructive">{updateCheck.warning}</div>
            )}
          </section>

          <section className="space-y-3 border-t pt-4">
            {data?.previousVersion && (
              <div className="text-xs text-muted-foreground">
                Previous version:
                <code className="font-mono">{data.previousVersion}</code>
                (one-click rollback available)
              </div>
            )}

            {data?.lastAppliedAt && (
              <div className="text-xs text-muted-foreground">
                Last updated at:
                <span className="font-mono">{formatDateTime(data.lastAppliedAt)}</span>
              </div>
            )}

            <div className="flex items-start justify-between gap-3">
              <div className="text-xs">
                <div className="font-medium text-foreground">Unattended auto-update</div>
                <div className="text-muted-foreground">
                  When enabled the service checks for new versions daily at the set time, and downloads the binary and restarts once a new version is found.
                </div>
              </div>
              <Switch
                checked={!!data?.autoApply}
                disabled={busy}
                onCheckedChange={(checked) => autoApplyMutation.mutate(checked)}
              />
            </div>

            <label
              className={`flex items-center justify-between gap-3 text-xs ${
                data?.autoApply ? '' : 'opacity-60'
              }`}
            >
              <span className="text-muted-foreground">Trigger time (local time zone,HH:MM)</span>
              <Input
                type="time"
                value={autoApplyTime}
                onChange={(e) => setAutoApplyTime(e.target.value)}
                onBlur={() => {
                  if (autoApplyTime && autoApplyTime !== data?.autoApplyTime) {
                    autoApplyTimeMutation.mutate(autoApplyTime)
                  }
                }}
                disabled={busy || !data?.autoApply}
                className="w-28 font-mono text-sm"
              />
            </label>
          </section>

          <section className="space-y-2 border-t pt-4">
            <div className="text-xs">
              <div className="flex items-center gap-1.5 font-medium text-foreground">
                <KeyRound className="h-3.5 w-3.5" />
                GitHub Token
                {data?.githubTokenSet && (
                  <Badge variant="success" className="ml-1">Configured</Badge>
                )}
              </div>
              <div className="text-muted-foreground">
                put GitHub API Rate limit from anonymous 60/hours raised to authenticated 5000/hours; read-only permission is enough.
              </div>
            </div>
            <div className="flex gap-2">
              <Input
                type="password"
                autoComplete="new-password"
                placeholder={
                  data?.githubTokenSet ? 'Saved (entering a new value overwrites it)' : 'ghp_xxxxxxxxxxxx'
                }
                value={githubToken}
                onChange={(e) => setGithubToken(e.target.value)}
                disabled={busy}
                className="font-mono text-sm"
              />
              <Button
                type="button"
                variant="outline"
                size="sm"
                disabled={busy || !githubToken.trim()}
                onClick={() => verifyTokenMutation.mutate(githubToken.trim())}
                title="Do not save, use this for now token Calls /rate_limit Test"
              >
                {verifyTokenMutation.isPending ? (
                  <RefreshCw className="h-3.5 w-3.5 animate-spin" />
                ) : (
                  <ShieldCheck className="h-3.5 w-3.5" />
                )}
                <span className="ml-1.5">Validate</span>
              </Button>
              <Button
                type="button"
                variant="outline"
                size="sm"
                disabled={busy || !githubToken.trim()}
                onClick={() => githubTokenMutation.mutate(githubToken.trim())}
              >
                {githubTokenMutation.isPending ? (
                  <RefreshCw className="h-3.5 w-3.5 animate-spin" />
                ) : (
                  <Save className="h-3.5 w-3.5" />
                )}
                <span className="ml-1.5">Save</span>
              </Button>
              {data?.githubTokenSet && (
                <Button
                  type="button"
                  variant="ghost"
                  size="sm"
                  disabled={busy}
                  onClick={() => githubTokenMutation.mutate('')}
                  title="Clear the saved GitHub Token"
                >
                  Clear
                </Button>
              )}
            </div>
            <RateLimitSummary
              info={rateLimit}
              loading={checkingRate}
              onRefresh={() =>
                queryClient.invalidateQueries({ queryKey: ['github-rate-limit'] })
              }
            />
          </section>

          {lastOutput && (
            <div className="rounded-md border bg-muted/40 p-3">
              <div className="mb-2 text-xs font-medium text-muted-foreground">Recent output</div>
              <pre className="max-h-48 overflow-auto whitespace-pre-wrap break-words text-xs">
                {lastOutput}
              </pre>
            </div>
          )}
        </div>

        <DialogFooter className="flex-wrap gap-2 sm:justify-between">
          <div className="flex flex-wrap gap-2">
            <Button
              type="button"
              variant="outline"
              disabled={busy}
              onClick={() => pullMutation.mutate()}
            >
              {pullMutation.isPending ? (
                <RefreshCw className="h-4 w-4 mr-2 animate-spin" />
              ) : (
                <Download className="h-4 w-4 mr-2" />
              )}
              Pull image
            </Button>
            <Button
              type="button"
              variant="outline"
              disabled={busy || !data?.previousVersion}
              onClick={() => rollbackMutation.mutate()}
              title={
                data?.previousVersion
                  ? `Roll back to ${data.previousVersion}`
                  : 'No rollback version recorded yet'
              }
            >
              {rollbackMutation.isPending ? (
                <RefreshCw className="h-4 w-4 mr-2 animate-spin" />
              ) : (
                <RotateCcw className="h-4 w-4 mr-2" />
              )}
              Roll back to the previous version
            </Button>
          </div>
          <Button
            type="button"
            disabled={busy || !updateCheck?.hasUpdate}
            onClick={() => applyMutation.mutate()}
            title={
              updateCheck?.hasUpdate
                ? `Update to v${updateCheck.latestVersion} and restart`
                : updateCheck?.currentVersion
                  ? `Already on the latest version v${updateCheck.currentVersion}`
                  : 'Checking for updates…'
            }
          >
            {applyMutation.isPending ? (
              <RefreshCw className="h-4 w-4 mr-2 animate-spin" />
            ) : (
              <UploadCloud className="h-4 w-4 mr-2" />
            )}
            Update and restart
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

interface ReleaseNotesPanelProps {
  version: string
  title?: string
  notes: string
  href?: string
}

/**
 * collapsible panel:displayCurrent versionof Changelog(GitHub Release body original text).
 * Uselightweight markdown the renderer restores the title/List/code block/linketc.,complex styling still clickable [in GitHub View] open.
 */
function ReleaseNotesPanel({ version, title, notes, href }: ReleaseNotesPanelProps) {
  const [open, setOpen] = useState(false)
  return (
    <div className="mt-3 rounded-md border bg-background/40">
      <button
        type="button"
        onClick={() => setOpen((v) => !v)}
        className="flex w-full items-center justify-between gap-2 px-3 py-2 text-xs font-medium text-foreground hover:bg-accent/40"
        aria-expanded={open}
      >
        <span className="flex items-center gap-2">
          <span>View v{version} Changelog</span>
          {title && (
            <span className="font-normal text-muted-foreground">— {title}</span>
          )}
        </span>
        <ChevronDown
          className={`h-4 w-4 shrink-0 transition-transform ${open ? 'rotate-180' : ''}`}
        />
      </button>
      {open && (
        <div className="border-t px-3 py-2.5 text-xs">
          <div className="max-h-72 overflow-auto pr-1 text-muted-foreground">
            <Markdown text={notes} />
          </div>
          {href && (
            <div className="mt-2">
              <a
                href={href}
                target="_blank"
                rel="noreferrer"
                className="inline-flex items-center gap-1 text-xs underline hover:no-underline"
              >
                <ExternalLink className="h-3 w-3" />
                in GitHub View full Release
              </a>
            </div>
          )}
        </div>
      )}
    </div>
  )
}

interface RateLimitSummaryProps {
  info: GitHubRateLimitInfo | undefined
  loading: boolean
  onRefresh: () => void
}

/**
 * GitHub API Rate limitsummaryCard:
 * - whether it carries token(authentication vs Anonymous)
 * - Used / limit / Remaining + progressitems
 * - Rate limitwindowResetTime(local timezone)
 * - Failedtimeput warning show it directly
 *
 * `/rate_limit` Endpointdoes not consume quota itself,safe to click [Refresh] button requery.
 */
function RateLimitSummary({ info, loading, onRefresh }: RateLimitSummaryProps) {
  if (loading && !info) {
    return (
      <div className="flex items-center gap-1.5 px-1 py-1 text-xs text-muted-foreground">
        <RefreshCw className="h-3.5 w-3.5 animate-spin" />
        Querying GitHub API Rate limit…
      </div>
    )
  }
  if (!info) return null

  const used = info.used ?? 0
  const limit = info.limit ?? 0
  const remaining = info.remaining ?? 0
  const ratio = limit > 0 ? Math.min(used / limit, 1) : 0
  const danger = info.valid && limit > 0 && remaining <= Math.max(5, Math.floor(limit / 20))
  const resetText = info.reset ? new Date(info.reset * 1000).toLocaleString() : '—'

  return (
    <div className="px-1 py-1 text-xs">
      <div className="flex items-center justify-between gap-2">
        <div className="flex items-center gap-1.5 font-medium text-foreground">
          {info.valid ? (
            <CheckCircle2 className="h-3.5 w-3.5 text-emerald-600" />
          ) : (
            <XCircle className="h-3.5 w-3.5 text-destructive" />
          )}
          API Rate limit
          <Badge variant={info.authenticated ? 'success' : 'secondary'} className="ml-1">
            {info.authenticated ? 'Authenticated' : 'Anonymous'}
          </Badge>
          {info.login && (
            <span className="ml-1 text-muted-foreground">@{info.login}</span>
          )}
        </div>
        <Button type="button" size="sm" variant="ghost" className="h-6 px-2" onClick={onRefresh}>
          <RefreshCw className={`h-3 w-3 ${loading ? 'animate-spin' : ''}`} />
        </Button>
      </div>

      {info.valid ? (
        <>
          <div className="mt-1.5 flex items-baseline gap-2 font-mono">
            <span className={danger ? 'text-amber-700 dark:text-amber-400' : ''}>
              Used {used} / {limit}
            </span>
            <span className="text-muted-foreground">·</span>
            <span>Remaining {remaining}</span>
          </div>
          <div className="mt-1 h-1.5 rounded-full bg-muted">
            <div
              className={`h-full rounded-full transition-all ${
                danger ? 'bg-amber-500' : 'bg-emerald-500'
              }`}
              style={{ width: `${ratio * 100}%` }}
            />
          </div>
          <div className="mt-1.5 text-muted-foreground">
            Reset at:<span className="font-mono">{resetText}</span>
          </div>
        </>
      ) : (
        <div className="mt-1.5 text-destructive">{info.warning || 'Token Validation failed'}</div>
      )}
    </div>
  )
}
