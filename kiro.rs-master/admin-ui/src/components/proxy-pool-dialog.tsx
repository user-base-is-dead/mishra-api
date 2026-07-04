import { useState } from 'react'
import { toast } from 'sonner'
import {
  Trash2,
  Plus,
  Upload,
  ToggleLeft,
  ToggleRight,
  Globe,
  Activity,
  Shuffle,
  CheckCircle2,
  XCircle,
  HelpCircle,
} from 'lucide-react'
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Badge } from '@/components/ui/badge'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import {
  getProxyPool,
  addProxy,
  batchAddProxies,
  deleteProxy,
  setProxyEnabled,
  getGlobalProxy,
  setGlobalProxy,
  checkProxy,
  checkAllProxies,
  assignProxiesRoundRobin,
} from '@/api/credentials'
import { extractErrorMessage, maskProxyUrl } from '@/lib/utils'
import type { ProxyPoolEntry } from '@/types/api'

interface ProxyPoolDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  /** click"assign"when the buttonofcallback(pass inProxy URL,Used forEdit credential) */
  onSelectProxy?: (url: string) => void
}


export function ProxyPoolDialog({ open, onOpenChange, onSelectProxy }: ProxyPoolDialogProps) {
  const [newUrl, setNewUrl] = useState('')
  const [newLabel, setNewLabel] = useState('')
  const [batchText, setBatchText] = useState('')
  const [showBatch, setShowBatch] = useState(false)
  const [batchErrors, setBatchErrors] = useState<string[]>([])
  const queryClient = useQueryClient()

  const { data, isLoading } = useQuery({
    queryKey: ['proxy-pool'],
    queryFn: getProxyPool,
    enabled: open,
  })

  const { data: globalProxyData } = useQuery({
    queryKey: ['global-proxy'],
    queryFn: getGlobalProxy,
    enabled: open,
  })

  const setGlobalProxyMutation = useMutation({
    mutationFn: (url: string | null) => setGlobalProxy({ proxyUrl: url }),
    onSuccess: (_, url) => {
      toast.success(url ? `Global proxy set: ${maskProxyUrl(url)}` : 'Global proxy cleared')
      queryClient.invalidateQueries({ queryKey: ['global-proxy'] })
    },
    onError: (err) => toast.error(`Operation failed: ${extractErrorMessage(err)}`),
  })

  const currentGlobalProxy = globalProxyData?.proxyUrl ?? null

  const addMutation = useMutation({
    mutationFn: () => addProxy({ url: newUrl.trim(), label: newLabel.trim() || undefined }),
    onSuccess: (entry) => {
      toast.success(`Proxy added:${entry.url}`)
      setNewUrl('')
      setNewLabel('')
      queryClient.invalidateQueries({ queryKey: ['proxy-pool'] })
    },
    onError: (err) => toast.error(`Add failed: ${extractErrorMessage(err)}`),
  })

  const batchMutation = useMutation({
    mutationFn: () =>
      batchAddProxies({
        urls: batchText.split('\n').map((l) => l.trim()).filter(Boolean),
      }),
    onSuccess: (res) => {
      if (res.errors === 0) {
        toast.success(`Bulk import complete: success ${res.added} items`)
      } else {
        toast.info(`Bulk import complete: success ${res.added} items, skipped ${res.errors} items`)
      }
      setBatchErrors(res.errorMessages)
      setBatchText('')
      queryClient.invalidateQueries({ queryKey: ['proxy-pool'] })
    },
    onError: (err) => toast.error(`Bulk import failed: ${extractErrorMessage(err)}`),
  })

  const deleteMutation = useMutation({
    mutationFn: (id: number) => deleteProxy(id),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['proxy-pool'] })
    },
    onError: (err) => toast.error(`Delete failed: ${extractErrorMessage(err)}`),
  })

  const toggleMutation = useMutation({
    mutationFn: ({ id, enabled }: { id: number; enabled: boolean }) =>
      setProxyEnabled(id, enabled),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['proxy-pool'] })
    },
    onError: (err) => toast.error(`Operation failed: ${extractErrorMessage(err)}`),
  })

  const [checkingId, setCheckingId] = useState<number | null>(null)
  const checkMutation = useMutation({
    mutationFn: (id: number) => checkProxy(id),
    onMutate: (id) => setCheckingId(id),
    onSuccess: (res) => {
      if (res.health === 'healthy') {
        toast.success(`Proxy available, latency ${res.latencyMs ?? '-'} ms`)
      } else {
        toast.error(res.autoDisabled ? 'Proxy probe failed, automatically disabled' : 'Proxy probe failed')
      }
      queryClient.invalidateQueries({ queryKey: ['proxy-pool'] })
    },
    onError: (err) => toast.error(`Probe failed: ${extractErrorMessage(err)}`),
    onSettled: () => setCheckingId(null),
  })

  const checkAllMutation = useMutation({
    mutationFn: () => checkAllProxies(),
    onSuccess: (res) => {
      toast.success(
        `Health check complete: healthy ${res.healthy}, error ${res.unhealthy}, automatically disabled ${res.autoDisabled}`
      )
      queryClient.invalidateQueries({ queryKey: ['proxy-pool'] })
    },
    onError: (err) => toast.error(`Check failed: ${extractErrorMessage(err)}`),
  })

  const assignRoundRobinMutation = useMutation({
    mutationFn: () => assignProxiesRoundRobin(null),
    onSuccess: (res) => {
      toast.success(`Used ${res.proxyCount} proxies distributed round-robin to ${res.assigned} credentials`)
      queryClient.invalidateQueries({ queryKey: ['proxy-pool'] })
      queryClient.invalidateQueries({ queryKey: ['credentials'] })
    },
    onError: (err) => toast.error(`Assignment failed: ${extractErrorMessage(err)}`),
  })

  const handleAdd = (e: React.FormEvent) => {
    e.preventDefault()
    if (!newUrl.trim()) return
    addMutation.mutate()
  }

  const renderHealthBadge = (proxy: ProxyPoolEntry) => {
    if (proxy.health === 'healthy') {
      return (
        <Badge variant="outline" className="text-xs gap-1 border-green-500/50 text-green-600 dark:text-green-400">
          <CheckCircle2 className="h-3 w-3" />
          {proxy.latencyMs != null ? `${proxy.latencyMs}ms` : 'Available'}
        </Badge>
      )
    }
    if (proxy.health === 'unhealthy') {
      return (
        <Badge variant="outline" className="text-xs gap-1 border-destructive/50 text-destructive">
          <XCircle className="h-3 w-3" />
          Error{proxy.consecutiveFailures > 0 ? ` ×${proxy.consecutiveFailures}` : ''}
        </Badge>
      )
    }
    return (
      <Badge variant="outline" className="text-xs gap-1 text-muted-foreground">
        <HelpCircle className="h-3 w-3" />
        Not checked
      </Badge>
    )
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-2xl max-h-[85vh] flex flex-col">
        <DialogHeader>
          <DialogTitle>Proxy IP Pool management</DialogTitle>
        </DialogHeader>

        <div className="flex-1 overflow-y-auto space-y-4 py-2">
          {/* singleitemsAdd */}
          {!showBatch && (
            <form onSubmit={handleAdd} className="flex gap-2">
              <Input
                placeholder="Proxy URL(such as socks5://user:pass@host:port)"
                value={newUrl}
                onChange={(e) => setNewUrl(e.target.value)}
                className="flex-1 font-mono text-sm"
              />
              <Input
                placeholder="Note (optional)"
                value={newLabel}
                onChange={(e) => setNewLabel(e.target.value)}
                className="w-32"
              />
              <Button type="submit" size="sm" disabled={addMutation.isPending || !newUrl.trim()}>
                <Plus className="h-4 w-4 mr-1" />
                Add
              </Button>
              <Button
                type="button"
                size="sm"
                variant="outline"
                onClick={() => setShowBatch(true)}
              >
                <Upload className="h-4 w-4 mr-1" />
                Bulk
              </Button>
            </form>
          )}

          {/* Bulk import */}
          {showBatch && (
            <div className="space-y-2">
              <label className="text-sm font-medium">
                Bulk import (one proxy per line URL,# starting with is a comment)
              </label>
              <textarea
                placeholder={'# One proxy per line URL\nsocks5://user:pass@host1:1080\nsocks5://user:pass@host2:1080\nhttp://user:pass@host3:8080'}
                value={batchText}
                onChange={(e) => setBatchText(e.target.value)}
                className="flex min-h-[120px] w-full rounded-md border border-input bg-background px-3 py-2 text-sm font-mono placeholder:text-muted-foreground focus-visible:outline-none focus-visible:border-ring focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ring/30"
              />
              <div className="flex gap-2">
                <Button
                  size="sm"
                  onClick={() => batchMutation.mutate()}
                  disabled={batchMutation.isPending || !batchText.trim()}
                >
                  Import
                </Button>
                <Button
                  size="sm"
                  variant="outline"
                  onClick={() => { setShowBatch(false); setBatchText(''); setBatchErrors([]) }}
                >
                  {batchMutation.isSuccess ? 'Close' : 'Cancel'}
                </Button>
              </div>
              {/* Bulk import failedDetails */}
              {batchErrors.length > 0 && (
                <div className="text-xs text-muted-foreground space-y-1 max-h-24 overflow-y-auto border rounded-md p-2">
                  <div className="font-medium text-yellow-600 dark:text-yellow-400">Skipped entries:</div>
                  {batchErrors.map((msg, i) => (
                    <div key={i}>{msg}</div>
                  ))}
                </div>
              )}
            </div>
          )}

          {/* Global proxyShow */}
          <div className="rounded-md border p-3 space-y-2">
            <div className="flex items-center justify-between">
              <div className="flex items-center gap-2">
                <Globe className="h-4 w-4 text-muted-foreground" />
                <span className="text-sm font-medium">Global proxy</span>
              </div>
              {currentGlobalProxy && (
                <Button
                  size="sm"
                  variant="ghost"
                  className="h-6 text-xs text-destructive hover:text-destructive"
                  onClick={() => setGlobalProxyMutation.mutate(null)}
                  disabled={setGlobalProxyMutation.isPending}
                >
                  Clear
                </Button>
              )}
            </div>
            <div className="text-xs font-mono text-muted-foreground">
              {currentGlobalProxy ? maskProxyUrl(currentGlobalProxy) : 'Not configured (direct connection)'}
            </div>
          </div>

          {/* ProxyList */}
          <div className="space-y-1">
            <div className="flex items-center justify-between">
              <div className="text-sm text-muted-foreground">
                total {data?.total ?? 0} proxies
              </div>
              {(data?.total ?? 0) > 0 && (
                <div className="flex items-center gap-1">
                  <Button
                    size="sm"
                    variant="outline"
                    className="h-7 text-xs"
                    onClick={() => checkAllMutation.mutate()}
                    disabled={checkAllMutation.isPending}
                    title="Run a health check on all enabled proxies"
                  >
                    <Activity className="h-3 w-3 mr-1" />
                    {checkAllMutation.isPending ? 'Checking...' : 'Check all'}
                  </Button>
                  <Button
                    size="sm"
                    variant="outline"
                    className="h-7 text-xs"
                    onClick={() => assignRoundRobinMutation.mutate()}
                    disabled={assignRoundRobinMutation.isPending}
                    title="Distribute available proxies round-robin to all credentials"
                  >
                    <Shuffle className="h-3 w-3 mr-1" />
                    Round-robin assign
                  </Button>
                </div>
              )}
            </div>

            {isLoading && (
              <div className="text-sm text-muted-foreground py-4 text-center">Loading...</div>
            )}

            {data?.proxies.length === 0 && !isLoading && (
              <div className="text-sm text-muted-foreground py-4 text-center">
                No proxies yet. Please add one
              </div>
            )}

            <div className="border rounded-md divide-y max-h-[320px] overflow-y-auto">
              {data?.proxies.map((proxy: ProxyPoolEntry) => (
                <div key={proxy.id} className="flex items-center gap-3 p-3">
                  <div className="flex-1 min-w-0">
                    <div className="flex items-center gap-2 flex-wrap">
                      <span className="font-mono text-xs truncate">
                        {maskProxyUrl(proxy.url)}
                      </span>
                      {proxy.label && (
                        <Badge variant="secondary" className="text-xs">{proxy.label}</Badge>
                      )}
                      {renderHealthBadge(proxy)}
                      {!proxy.enabled && (
                        <Badge variant="outline" className="text-xs text-muted-foreground">
                          {proxy.autoDisabled ? 'Auto-disable' : 'Disabled'}
                        </Badge>
                      )}
                    </div>
                    <div className="flex items-center gap-3 mt-0.5">
                      {proxy.credentialCount > 0 && (
                        <span className="text-xs text-muted-foreground">
                          {proxy.credentialCount} credentials in use
                        </span>
                      )}
                      {proxy.lastCheckedAt && (
                        <span className="text-xs text-muted-foreground">
                          Checked at {new Date(proxy.lastCheckedAt).toLocaleString()}
                        </span>
                      )}
                    </div>
                  </div>
                  <div className="flex items-center gap-1 shrink-0">
                    <Button
                      size="sm"
                      variant="outline"
                      className="h-7 text-xs"
                      onClick={() => checkMutation.mutate(proxy.id)}
                      disabled={checkingId === proxy.id}
                      title="Test this proxy connectivity"
                    >
                      <Activity className="h-3 w-3 mr-1" />
                      {checkingId === proxy.id ? 'Testing' : 'Test'}
                    </Button>
                    {onSelectProxy && proxy.enabled && (
                      <Button
                        size="sm"
                        variant="outline"
                        className="h-7 text-xs"
                        onClick={() => {
                          onSelectProxy(proxy.url)
                          onOpenChange(false)
                        }}
                      >
                        Select
                      </Button>
                    )}
                    {proxy.enabled && proxy.url !== currentGlobalProxy && (
                      <Button
                        size="sm"
                        variant="outline"
                        className="h-7 text-xs"
                        onClick={() => setGlobalProxyMutation.mutate(proxy.url)}
                        disabled={setGlobalProxyMutation.isPending}
                        title="Set as global proxy"
                      >
                        <Globe className="h-3 w-3 mr-1" />
                        Global
                      </Button>
                    )}
                    {proxy.url === currentGlobalProxy && (
                      <Badge variant="secondary" className="text-xs h-7">Global</Badge>
                    )}
                    <Button
                      size="sm"
                      variant="ghost"
                      className="h-7 w-7 p-0"
                      onClick={() => toggleMutation.mutate({ id: proxy.id, enabled: !proxy.enabled })}
                      title={proxy.enabled ? 'Disable this proxy' : 'Enable this proxy'}
                    >
                      {proxy.enabled ? (
                        <ToggleRight className="h-4 w-4 text-green-500" />
                      ) : (
                        <ToggleLeft className="h-4 w-4 text-muted-foreground" />
                      )}
                    </Button>
                    <Button
                      size="sm"
                      variant="ghost"
                      className="h-7 w-7 p-0 text-destructive hover:text-destructive"
                      onClick={() => deleteMutation.mutate(proxy.id)}
                      disabled={deleteMutation.isPending}
                    >
                      <Trash2 className="h-4 w-4" />
                    </Button>
                  </div>
                </div>
              ))}
            </div>
          </div>
        </div>
      </DialogContent>
    </Dialog>
  )
}
