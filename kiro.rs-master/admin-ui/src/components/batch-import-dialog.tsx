import { useRef, useState } from 'react'
import { toast } from 'sonner'
import { useQuery, useQueryClient } from '@tanstack/react-query'
import { CheckCircle2, XCircle, AlertCircle, Loader2 } from 'lucide-react'
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
} from '@/components/ui/dialog'
import { Button } from '@/components/ui/button'
import { useCredentials } from '@/hooks/use-credentials'
import {
  batchImportCredentials,
  getProxyPool,
  type BatchImportItemEvent,
  type BatchImportSummary,
} from '@/api/credentials'
import type { AddCredentialRequest } from '@/types/api'
import { extractErrorMessage, sha256Hex } from '@/lib/utils'

interface BatchImportDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
}

interface CredentialInput {
  refreshToken?: string
  clientId?: string
  clientSecret?: string
  region?: string
  authRegion?: string
  apiRegion?: string
  priority?: number
  machineId?: string
  kiroApiKey?: string
  authMethod?: string
  endpoint?: string
  email?: string
  proxyUrl?: string
  proxyUsername?: string
  proxyPassword?: string
}

interface VerificationResult {
  index: number
  status: 'pending' | 'checking' | 'verifying' | 'verified' | 'imported' | 'duplicate' | 'failed'
  error?: string
  usage?: string
  email?: string
  credentialId?: number
  rollbackStatus?: 'success' | 'failed' | 'skipped'
  rollbackError?: string
}



export function BatchImportDialog({ open, onOpenChange }: BatchImportDialogProps) {
  const [jsonInput, setJsonInput] = useState('')
  const [importing, setImporting] = useState(false)
  const [progress, setProgress] = useState({ current: 0, total: 0 })
  const [currentProcessing, setCurrentProcessing] = useState<string>('')
  const [results, setResults] = useState<VerificationResult[]>([])
  // in progressof AbortController,Used for"Stop import":abort will make fetch Stream interrupted,
  // serverindowntimesdetected the receiver when writing back the eventClosenamelyStophandleRemainingCredential.
  const abortRef = useRef<AbortController | null>(null)

  const { data: existingCredentials } = useCredentials()
  const queryClient = useQueryClient()
  const { data: proxyPool } = useQuery({
    queryKey: ['proxy-pool'],
    queryFn: getProxyPool,
    enabled: open,
  })

  const resetForm = () => {
    setJsonInput('')
    setProgress({ current: 0, total: 0 })
    setCurrentProcessing('')
    setResults([])
  }

  // update a single row result in place by original index(avoid everyitemsbeyond a full copyofextra complexity)
  const updateResult = (i: number, patch: Partial<VerificationResult>) => {
    setResults(prev => {
      const next = [...prev]
      next[i] = { ...next[i], ...patch }
      return next
    })
  }

  const handleBatchImport = async (verify: boolean) => {
    // parse separately first JSON,give preciseofErrorhint
    let credentials: CredentialInput[]
    try {
      const parsed = JSON.parse(jsonInput)
      credentials = Array.isArray(parsed) ? parsed : [parsed]
    } catch (error) {
      toast.error('JSON Format error: ' + extractErrorMessage(error))
      return
    }

    if (credentials.length === 0) {
      toast.error('No credentials to import')
      return
    }

    try {
      setImporting(true)
      setProgress({ current: 0, total: credentials.length })

      // initialize the result
      const initialResults: VerificationResult[] = credentials.map((_, i) => ({
        index: i + 1,
        status: 'pending'
      }))
      setResults(initialResults)

      // Client keydedupe:OAuth and API Key separatelyUsecorrespondingof hash set
      const existingOauthHashes = new Set(
        existingCredentials?.credentials
          .map(c => c.refreshTokenHash)
          .filter((hash): hash is string => Boolean(hash)) || []
      )
      const existingApiKeyHashes = new Set(
        existingCredentials?.credentials
          .map(c => c.apiKeyHash)
          .filter((hash): hash is string => Boolean(hash)) || []
      )

      // AvailableofProxy poolitemsitem(Used fornoneProxyCredentialofrandom assignment)
      const enabledProxies = proxyPool?.proxies.filter(p => p.enabled) ?? []

      // local preprocessing:Proxyassign + dedupe + validate + constructRequest.
      // notviaofmark the row as terminal directly;viaofcollect into toImport,record its original index,
      // so thatputserver SSE event(by toImport position insideBack index)map back to the matching row.
      const toImport: { index: number; req: AddCredentialRequest }[] = []

      for (let i = 0; i < credentials.length; i++) {
        const cred = credentials[i]

        // ifCredentialnot specifiedProxyandProxy poolhasAvailableProxy,randomly assign oneitems
        if (!cred.proxyUrl?.trim() && enabledProxies.length > 0) {
          const picked = enabledProxies[Math.floor(Math.random() * enabledProxies.length)]
          cred.proxyUrl = picked.url
        }
        const isApiKeyCred = !!(cred.kiroApiKey?.trim()) || cred.authMethod === 'api_key'

        updateResult(i, { status: 'checking' })

        if (isApiKeyCred) {
          const apiKey = cred.kiroApiKey?.trim() || ''
          if (!apiKey) {
            updateResult(i, { status: 'failed', error: 'Missing kiroApiKey' })
            continue
          }
          const credHash = await sha256Hex(apiKey)
          if (existingApiKeyHashes.has(credHash)) {
            const existingCred = existingCredentials?.credentials.find(c => c.apiKeyHash === credHash)
            updateResult(i, {
              status: 'duplicate',
              error: 'This credential already exists',
              email: existingCred?.email || undefined
            })
            continue
          }
          existingApiKeyHashes.add(credHash)
          toImport.push({
            index: i,
            req: {
              authMethod: 'api_key',
              kiroApiKey: apiKey,
              priority: cred.priority || 0,
              authRegion: cred.authRegion?.trim() || cred.region?.trim() || undefined,
              apiRegion: cred.apiRegion?.trim() || undefined,
              machineId: cred.machineId?.trim() || undefined,
              endpoint: cred.endpoint?.trim() || undefined,
              email: cred.email?.trim() || undefined,
              proxyUrl: cred.proxyUrl?.trim() || undefined,
              proxyUsername: cred.proxyUsername?.trim() || undefined,
              proxyPassword: cred.proxyPassword?.trim() || undefined,
            },
          })
        } else {
          const token = cred.refreshToken?.trim() || ''
          if (!token) {
            updateResult(i, { status: 'failed', error: 'Missing refreshToken' })
            continue
          }
          const credHash = await sha256Hex(token)
          if (existingOauthHashes.has(credHash)) {
            const existingCred = existingCredentials?.credentials.find(c => c.refreshTokenHash === credHash)
            updateResult(i, {
              status: 'duplicate',
              error: 'This credential already exists',
              email: existingCred?.email || undefined
            })
            continue
          }
          existingOauthHashes.add(credHash)

          const clientId = cred.clientId?.trim() || undefined
          const clientSecret = cred.clientSecret?.trim() || undefined
          const authMethod = clientId && clientSecret ? 'idc' : 'social'
          if (authMethod === 'social' && (clientId || clientSecret)) {
            updateResult(i, {
              status: 'failed',
              error: 'idc This mode requires providing both clientId and clientSecret',
            })
            continue
          }

          toImport.push({
            index: i,
            req: {
              refreshToken: token,
              authMethod,
              authRegion: cred.authRegion?.trim() || cred.region?.trim() || undefined,
              apiRegion: cred.apiRegion?.trim() || undefined,
              clientId,
              clientSecret,
              priority: cred.priority || 0,
              machineId: cred.machineId?.trim() || undefined,
              endpoint: cred.endpoint?.trim() || undefined,
              email: cred.email?.trim() || undefined,
              proxyUrl: cred.proxyUrl?.trim() || undefined,
              proxyUsername: cred.proxyUsername?.trim() || undefined,
              proxyPassword: cred.proxyPassword?.trim() || undefined,
            },
          })
        }
      }

      // pending uploadofrow markerasValidating
      for (const item of toImport) {
        updateResult(item.index, { status: 'verifying' })
      }

      if (toImport.length === 0) {
        setCurrentProcessing('No credentials to upload (all were duplicated or failed validation)')
      } else {
        setCurrentProcessing(
          `${verify ? 'Bulk validate' : 'Import directly'}in (${toImport.length} )…`,
        )
        // onetimesattribute POST,serverhasconcurrent processing,one by oneitemsvia SSE return the result.
        // event ev.index is toImport insideofposition,needs mapping back to the originalCredentialindex.
        const controller = new AbortController()
        abortRef.current = controller
        await batchImportCredentials(
          { credentials: toImport.map(t => t.req), concurrency: 8, verify },
          (ev: BatchImportItemEvent) => {
            const orig = toImport[ev.index]?.index ?? -1
            if (orig < 0) return
            if (ev.status === 'verified') {
              updateResult(orig, {
                status: 'verified',
                usage: ev.usage,
                email: ev.email,
                credentialId: ev.credentialId,
              })
              setCurrentProcessing(ev.email ? `Validation succeeded: ${ev.email}` : 'Validation succeeded')
            } else if (ev.status === 'imported') {
              updateResult(orig, {
                status: 'imported',
                email: ev.email,
                credentialId: ev.credentialId,
              })
              setCurrentProcessing(ev.email ? `Imported: ${ev.email}` : 'Imported')
            } else if (ev.status === 'duplicate') {
              updateResult(orig, { status: 'duplicate', error: ev.error || 'This credential already exists' })
            } else {
              updateResult(orig, {
                status: 'failed',
                error: ev.error,
                rollbackStatus: ev.rolledBack ? 'success' : undefined,
              })
            }
          },
          (s: BatchImportSummary) => {
            const importedTotal = s.imported + s.verified
            if (verify) {
              if (s.failed === 0 && s.duplicate === 0) {
                toast.success(`Successfully imported and validated ${s.verified} credentials`)
              } else {
                toast.info(
                  `Validation complete: success ${s.verified} succeeded, duplicated ${s.duplicate} succeeded, failed ${s.failed} items (excluded ${s.rolledBack})`
                )
                if (s.rolledBack < s.failed) {
                  toast.warning(`has ${s.failed - s.rolledBack} failed credentials were not rolled back. Please handle them manually`)
                }
              }
            } else {
              if (s.failed === 0 && s.duplicate === 0) {
                toast.success(`Import directly ${importedTotal} credentials (not validated)`)
              } else {
                toast.info(
                  `Import complete: success ${importedTotal} succeeded, duplicated ${s.duplicate} succeeded, failed ${s.failed} items`
                )
              }
            }
          },
          controller.signal,
        )
      }

      // RefreshCredential list,let the newImportofvisible immediately
      await queryClient.invalidateQueries({ queryKey: ['credentials'] })
    } catch (error) {
      // user click"Stop"→ AbortError,the server willStophandleRemainingCredential;completedofkeep.
      if (error instanceof DOMException && error.name === 'AbortError') {
        toast.info('Import stopped (completed credentials are kept)')
        await queryClient.invalidateQueries({ queryKey: ['credentials'] })
      } else {
        toast.error('Import failed: ' + extractErrorMessage(error))
      }
    } finally {
      abortRef.current = null
      setImporting(false)
    }
  }

  const getStatusIcon = (status: VerificationResult['status']) => {
    switch (status) {
      case 'pending':
        return <div className="w-5 h-5 rounded-full border-2 border-gray-300" />
      case 'checking':
      case 'verifying':
        return <Loader2 className="w-5 h-5 animate-spin text-blue-500" />
      case 'verified':
        return <CheckCircle2 className="w-5 h-5 text-green-500" />
      case 'imported':
        return <CheckCircle2 className="w-5 h-5 text-sky-500" />
      case 'duplicate':
        return <AlertCircle className="w-5 h-5 text-yellow-500" />
      case 'failed':
        return <XCircle className="w-5 h-5 text-red-500" />
    }
  }

  const getStatusText = (result: VerificationResult) => {
    switch (result.status) {
      case 'pending':
        return 'Waiting'
      case 'checking':
        return 'Check for duplicates...'
      case 'verifying':
        return 'Processing...'
      case 'verified':
        return 'Validation succeeded'
      case 'imported':
        return 'Imported (not validated)'
      case 'duplicate':
        return 'Duplicate credential'
      case 'failed':
        if (result.rollbackStatus === 'success') return 'Validation failed (excluded)'
        if (result.rollbackStatus === 'failed') return 'Validation failed (not excluded)'
        return 'Processing failed (not created)'
    }
  }

  // terminated(verified/imported/duplicate/failed)ofrow count,drive the progressitems;Client keydedupe/validatein
  // done before upload,so these rowsin SSE streamStartalready counted before.
  const finalizedCount = results.filter(
    r =>
      r.status === 'verified' ||
      r.status === 'imported' ||
      r.status === 'duplicate' ||
      r.status === 'failed'
  ).length

  return (
    <Dialog
      open={open}
      onOpenChange={(newOpen) => {
        if (!newOpen) {
          if (importing) {
            // Importduring the processClose = Stop import(abort server stream)
            abortRef.current?.abort()
          } else {
            resetForm()
          }
        }
        onOpenChange(newOpen)
      }}
    >
      <DialogContent className="sm:max-w-2xl max-h-[80vh] flex flex-col">
        <DialogHeader>
          <DialogTitle>Bulk import credentials</DialogTitle>
        </DialogHeader>

        <div className="flex-1 overflow-y-auto space-y-4 py-4">
          <div className="space-y-2">
            <label className="text-sm font-medium">
              JSON format credential
            </label>
            <textarea
              placeholder={'Paste JSON format credentials (single object or array supported)\n\nOAuth: [{"refreshToken":"...","clientId":"...","clientSecret":"..."}]\nAPI Key: [{"kiroApiKey":"ksk_xxx"}]\n\nSupported region field is mapped automatically to authRegion'}
              value={jsonInput}
              onChange={(e) => setJsonInput(e.target.value)}
              disabled={importing}
              className="flex min-h-[200px] w-full rounded-xl border border-input bg-background/60 px-3.5 py-2.5 text-sm transition-[border-color,background-color,box-shadow] duration-150 ease-apple placeholder:text-muted-foreground/70 hover:border-border focus-visible:outline-none focus-visible:border-ring focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ring/30 focus-visible:bg-background disabled:cursor-not-allowed disabled:opacity-50 font-mono"
            />
            <p className="text-xs text-muted-foreground">
              💡 "Start import and validation"validates the balance and excludes failures automatically;"Import directly"Store only without validation (faster). Both modes support mid-run"Stop".
            </p>
          </div>

          {(importing || results.length > 0) && (
            <>
              {/* progressitems */}
              <div className="space-y-2">
                <div className="flex justify-between text-sm">
                  <span>{importing ? 'Validation progress' : 'Validation complete'}</span>
                  <span>{finalizedCount} / {progress.total}</span>
                </div>
                <div className="w-full bg-secondary rounded-full h-2">
                  <div
                    className="bg-primary h-2 rounded-full transition-all"
                    style={{ width: `${progress.total > 0 ? (finalizedCount / progress.total) * 100 : 0}%` }}
                  />
                </div>
                {importing && currentProcessing && (
                  <div className="text-xs text-muted-foreground">
                    {currentProcessing}
                  </div>
                )}
              </div>

              {/* statistics */}
              <div className="flex gap-4 text-sm">
                <span className="text-green-600 dark:text-green-400">
                  ✓ Validation succeeded: {results.filter(r => r.status === 'verified').length}
                </span>
                <span className="text-sky-600 dark:text-sky-400">
                  ✓ Imported: {results.filter(r => r.status === 'imported').length}
                </span>
                <span className="text-yellow-600 dark:text-yellow-400">
                  ⚠ Duplicate: {results.filter(r => r.status === 'duplicate').length}
                </span>
                <span className="text-red-600 dark:text-red-400">
                  ✗ Failed: {results.filter(r => r.status === 'failed').length}
                </span>
              </div>

              {/* resultList */}
              <div className="border rounded-md divide-y max-h-[300px] overflow-y-auto">
                {results.map((result) => (
                  <div key={result.index} className="p-3">
                    <div className="flex items-start gap-3">
                      {getStatusIcon(result.status)}
                      <div className="flex-1 min-w-0">
                        <div className="flex items-center gap-2">
                          <span className="text-sm font-medium">
                            {result.email || `Credential #${result.index}`}
                          </span>
                          <span className="text-xs text-muted-foreground">
                            {getStatusText(result)}
                          </span>
                        </div>
                        {result.usage && (
                          <div className="text-xs text-muted-foreground mt-1">
                            Usage: {result.usage}
                          </div>
                        )}
                        {result.error && (
                          <div className="text-xs text-red-600 dark:text-red-400 mt-1">
                            {result.error}
                          </div>
                        )}
                        {result.rollbackError && (
                          <div className="text-xs text-red-600 dark:text-red-400 mt-1">
                            Rollback failed: {result.rollbackError}
                          </div>
                        )}
                      </div>
                    </div>
                  </div>
                ))}
              </div>
            </>
          )}
        </div>

        <DialogFooter>
          {importing ? (
            <Button
              type="button"
              variant="destructive"
              onClick={() => abortRef.current?.abort()}
            >
              Stop import
            </Button>
          ) : (
            <>
              <Button
                type="button"
                variant="outline"
                onClick={() => {
                  onOpenChange(false)
                  resetForm()
                }}
              >
                {results.length > 0 ? 'Close' : 'Cancel'}
              </Button>
              {results.length === 0 && (
                <>
                  <Button
                    type="button"
                    variant="outline"
                    onClick={() => handleBatchImport(false)}
                    disabled={!jsonInput.trim()}
                  >
                    Import directly (without validation)
                  </Button>
                  <Button
                    type="button"
                    onClick={() => handleBatchImport(true)}
                    disabled={!jsonInput.trim()}
                  >
                    Start import and validation
                  </Button>
                </>
              )}
            </>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
