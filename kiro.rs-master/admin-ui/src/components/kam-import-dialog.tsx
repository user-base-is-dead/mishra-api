import { useState, useMemo, useRef } from 'react'
import { toast } from 'sonner'
import { useQuery, useQueryClient } from '@tanstack/react-query'
import { CheckCircle2, XCircle, AlertCircle, Loader2, Upload } from 'lucide-react'
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

interface KamImportDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
}

// KAM Export JSON inaccountsStructure
interface KamAccount {
  email?: string
  userId?: string | null
  nickname?: string
  idp?: string
  credentials: {
    refreshToken: string
    accessToken?: string
    profileArn?: string
    // KAM 1.6.9+ new versionExportasmillisecondTimetimestampdigits,old versionas RFC3339 string
    expiresAt?: string | number
    clientId?: string
    clientSecret?: string
    region?: string
    authMethod?: string
    provider?: string
    startUrl?: string
  }
  machineId?: string
  status?: string
}

// put KAM of expiresAt normalize fields uniformlyas RFC3339 string
// - number(millisecondTimetimestamp)→ convert ISO string
// - string → trim afterBack,treat empty string asas undefined
// - other → undefined
function normalizeExpiresAt(value: unknown): string | undefined {
  if (typeof value === 'number' && Number.isFinite(value)) {
    const date = new Date(value)
    return Number.isNaN(date.getTime()) ? undefined : date.toISOString()
  }
  if (typeof value === 'string') {
    const trimmed = value.trim()
    return trimmed.length > 0 ? trimmed : undefined
  }
  return undefined
}

interface VerificationResult {
  index: number
  status: 'pending' | 'checking' | 'verifying' | 'verified' | 'imported' | 'duplicate' | 'failed' | 'skipped'
  error?: string
  usage?: string
  email?: string
  credentialId?: number
  rollbackStatus?: 'success' | 'failed' | 'skipped'
  rollbackError?: string
}



// compatible KAM 1.8.3 new version flat layoutFormat,uniform conversionasoldFormat(credentials nestedStructure)
function normalizeKamAccount(item: unknown): unknown {
  if (typeof item !== 'object' || item === null) return item
  const obj = item as Record<string, unknown>
  // newFormat:refreshToken directlyinAccounton the object,none credentials nested
  if (typeof obj.refreshToken === 'string' && typeof obj.credentials === 'undefined') {
    const email = typeof obj.email === 'string' ? obj.email : undefined
    const userId =
      typeof obj.userId === 'string' || obj.userId === null ? (obj.userId as string | null) : undefined
    const nickname =
      typeof obj.nickname === 'string'
        ? obj.nickname
        : typeof obj.label === 'string'
          ? (obj.label as string)
          : undefined
    const status = typeof obj.status === 'string' ? obj.status : undefined
    const idp = typeof obj.idp === 'string' ? obj.idp : undefined
    const machineId = typeof obj.machineId === 'string' ? obj.machineId : undefined
    const accessToken = typeof obj.accessToken === 'string' ? obj.accessToken : undefined
    const profileArn = typeof obj.profileArn === 'string' ? obj.profileArn : undefined
    const expiresAt =
      typeof obj.expiresAt === 'string' || typeof obj.expiresAt === 'number'
        ? (obj.expiresAt as string | number)
        : undefined
    const clientId = typeof obj.clientId === 'string' ? obj.clientId : undefined
    const clientSecret = typeof obj.clientSecret === 'string' ? obj.clientSecret : undefined
    const region = typeof obj.region === 'string' ? obj.region : undefined
    const authMethod = typeof obj.authMethod === 'string' ? obj.authMethod : undefined
    const provider = typeof obj.provider === 'string' ? obj.provider : undefined
    const startUrl = typeof obj.startUrl === 'string' ? obj.startUrl : undefined

    return {
      email,
      userId,
      nickname,
      idp,
      status,
      machineId,
      credentials: {
        refreshToken: obj.refreshToken,
        accessToken,
        profileArn,
        expiresAt,
        clientId,
        clientSecret,
        region,
        authMethod,
        provider,
        startUrl,
      },
    }
  }
  return item
}

// check whether the elementashaseffectof KAM AccountStructure
function isValidKamAccount(item: unknown): item is KamAccount {
  if (typeof item !== 'object' || item === null) return false
  const obj = item as Record<string, unknown>
  if (typeof obj.credentials !== 'object' || obj.credentials === null) return false
  const cred = obj.credentials as Record<string, unknown>
  return typeof cred.refreshToken === 'string' && cred.refreshToken.trim().length > 0
}

// parse KAM Export JSON,SupportedsingleAccountandmanyAccountFormat
function parseKamJson(raw: string): KamAccount[] {
  const parsed = JSON.parse(raw)

  let rawItems: unknown[]

  // standard KAM ExportFormat:{ version, accounts: [...] }
  if (parsed.accounts && Array.isArray(parsed.accounts)) {
    rawItems = parsed.accounts
  }
  // direct array(including KAM 1.8.3 new version flat layoutFormat)
  else if (Array.isArray(parsed)) {
    rawItems = parsed
  }
  // singleaccountsobject(oldFormat,has credentials field)
  else if (parsed.credentials && typeof parsed.credentials === 'object') {
    rawItems = [parsed]
  }
  // singleaccountsobject(newFormat,refreshToken flat layout)
  else if (typeof parsed.refreshToken === 'string') {
    rawItems = [parsed]
  }
  else {
    throw new Error('Unrecognized KAM JSON Format')
  }

  // compatible with the newFormat: willflat layoutAccountuniform conversionas credentials nestedStructure
  const normalizedItems = rawItems.map(normalizeKamAccount)
  const validAccounts = normalizedItems.filter(isValidKamAccount)

  if (rawItems.length > 0 && validAccounts.length === 0) {
    throw new Error(`total ${rawItems.length} records, but all lack a valid credentials.refreshToken`)
  }

  if (validAccounts.length < rawItems.length) {
    const skipped = rawItems.length - validAccounts.length
    console.warn(`KAM Import: skipped ${skipped} entries lack a valid credentials.refreshToken records of`)
  }

  return validAccounts
}

export function KamImportDialog({ open, onOpenChange }: KamImportDialogProps) {
  const [jsonInput, setJsonInput] = useState('')
  const [importing, setImporting] = useState(false)
  const [skipErrorAccounts, setSkipErrorAccounts] = useState(true)
  const [progress, setProgress] = useState({ current: 0, total: 0 })
  const [currentProcessing, setCurrentProcessing] = useState<string>('')
  const [results, setResults] = useState<VerificationResult[]>([])
  const fileInputRef = useRef<HTMLInputElement>(null)
  // in progressof AbortController,Used for"Stop import":abort let fetch Stream interrupted,
  // serverindowntimesdetected the receiver when writing back the eventClosenamelyStophandleRemainingAccount.
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
    if (fileInputRef.current) fileInputRef.current.value = ''
  }

  // update a single row result in place by original index
  const updateResult = (i: number, patch: Partial<VerificationResult>) => {
    setResults(prev => {
      const next = [...prev]
      next[i] = { ...next[i], ...patch }
      return next
    })
  }

  const handleFileSelect = async (event: React.ChangeEvent<HTMLInputElement>) => {
    const files = Array.from(event.target.files ?? [])
    if (files.length === 0) return

    try {
      // read allhasthe file and merge accounts,keep each version meta info for troubleshooting
      const fileTexts = await Promise.all(
        files.map(async (f) => ({ name: f.name, text: await f.text() }))
      )

      const merged: unknown[] = []
      const failed: { name: string; reason: string }[] = []

      for (const { name, text } of fileTexts) {
        try {
          const parsed = JSON.parse(text)
          if (parsed && Array.isArray(parsed.accounts)) {
            merged.push(...parsed.accounts)
          } else if (Array.isArray(parsed)) {
            merged.push(...parsed)
          } else if (parsed && typeof parsed === 'object') {
            // singleAccountobject(new/oldFormat)
            merged.push(parsed)
          } else {
            failed.push({ name, reason: 'Unrecognized JSON Structure' })
          }
        } catch (e) {
          failed.push({ name, reason: extractErrorMessage(e) })
        }
      }

      if (merged.length === 0) {
        toast.error(`All files failed to parse:${failed.map((f) => `${f.name}(${f.reason})`).join(';')}`)
        return
      }

      // after merge use a uniformFormatOutput,reuse textarea currenthasofparse and preview logic
      const mergedJson = JSON.stringify({ version: 'merged', accounts: merged }, null, 2)
      setJsonInput(mergedJson)
      setResults([])

      const fileSummary = files.length === 1 ? files[0].name : `${files.length} files`
      if (failed.length > 0) {
        toast.warning(
          `Loaded ${fileSummary}, merged ${merged.length} records;${failed.length} files failed to parse:${failed.map((f) => f.name).join(',')}`
        )
      } else {
        toast.success(`Loaded ${fileSummary}, merged ${merged.length} records`)
      }
    } catch (error) {
      toast.error('Read file failed: ' + extractErrorMessage(error))
    } finally {
      // clear value so that againtimesselecting a file with the same name also triggers onChange
      event.target.value = ''
    }
  }

  const handleImport = async (verify: boolean) => {
    // parse separately first JSON,give preciseofErrorhint
    let validAccounts: KamAccount[]
    try {
      const accounts = parseKamJson(jsonInput)

      if (accounts.length === 0) {
        toast.error('No accounts to import')
        return
      }

      validAccounts = accounts.filter(a => a.credentials?.refreshToken)
      if (validAccounts.length === 0) {
        toast.error('does not contain a valid refreshToken accounts')
        return
      }
    } catch (error) {
      toast.error('JSON Format error: ' + extractErrorMessage(error))
      return
    }

    try {
      setImporting(true)
      setProgress({ current: 0, total: validAccounts.length })

      // initialize the result,mark error accounts in statusas skipped(do not upload)
      const initialResults: VerificationResult[] = validAccounts.map((account, i) => {
        if (skipErrorAccounts && account.status === 'error') {
          return { index: i + 1, status: 'skipped' as const, email: account.email || account.nickname }
        }
        return { index: i + 1, status: 'pending' as const, email: account.email || account.nickname }
      })
      setResults(initialResults)

      // Client keydedupe
      const existingTokenHashes = new Set(
        existingCredentials?.credentials
          .map(c => c.refreshTokenHash)
          .filter((hash): hash is string => Boolean(hash)) || []
      )

      const enabledProxies = proxyPool?.proxies.filter(p => p.enabled) ?? []

      // local preprocessing:Skip error Account,dedupe,validate,constructRequest.
      // viaofcollect into toImport(record the original index),notviaofmark the row as terminal directly.
      const toImport: { index: number; req: AddCredentialRequest }[] = []

      for (let i = 0; i < validAccounts.length; i++) {
        const account = validAccounts[i]

        // Skip error accounts in status(initialResults already marked inside skipped)
        if (skipErrorAccounts && account.status === 'error') {
          continue
        }

        const cred = account.credentials
        const token = cred.refreshToken.trim()
        const tokenHash = await sha256Hex(token)

        updateResult(i, { status: 'checking' })

        // Check for duplicates
        if (existingTokenHashes.has(tokenHash)) {
          const existingCred = existingCredentials?.credentials.find(c => c.refreshTokenHash === tokenHash)
          updateResult(i, {
            status: 'duplicate',
            error: 'This credential already exists',
            email: existingCred?.email || account.email,
          })
          continue
        }
        existingTokenHashes.add(tokenHash)

        const clientId = cred.clientId?.trim() || undefined
        const clientSecret = cred.clientSecret?.trim() || undefined
        const authMethod = clientId && clientSecret ? 'idc' : 'social'
        const provider = cred.provider?.trim() || account.idp?.trim() || undefined

        // idc in this mode both must be provided clientId and clientSecret
        if (authMethod === 'social' && (clientId || clientSecret)) {
          updateResult(i, { status: 'failed', error: 'idc This mode requires providing both clientId and clientSecret' })
          continue
        }

        // KAM Accountnone proxyUrl field,noneProxytimefromrandomly assign one from the poolitems
        const proxyUrl = enabledProxies.length > 0
          ? enabledProxies[Math.floor(Math.random() * enabledProxies.length)].url
          : undefined

        toImport.push({
          index: i,
          req: {
            refreshToken: token,
            accessToken: cred.accessToken?.trim() || undefined,
            profileArn: cred.profileArn?.trim() || undefined,
            expiresAt: normalizeExpiresAt(cred.expiresAt),
            authMethod,
            provider,
            authRegion: cred.region?.trim() || undefined,
            startUrl: cred.startUrl?.trim() || undefined,
            clientId,
            clientSecret,
            machineId: account.machineId?.trim() || undefined,
            email: account.email?.trim() || undefined,
            proxyUrl,
          },
        })
      }

      // pending uploadofrow markerasProcessing
      for (const item of toImport) {
        updateResult(item.index, { status: 'verifying' })
      }

      if (toImport.length === 0) {
        setCurrentProcessing('No accounts to upload (all were skipped, duplicated, or failed validation)')
      } else {
        setCurrentProcessing(
          `${verify ? 'Bulk validate' : 'Import directly'}in (${toImport.length} )…`,
        )
        // onetimesattribute POST,serverhasconcurrent processing,one by oneitemsvia SSE return the result.
        // event ev.index is toImport insideofposition,needs mapping back to the originalAccountindex.
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

      // RefreshCredential list
      await queryClient.invalidateQueries({ queryKey: ['credentials'] })
    } catch (error) {
      // user click"Stop"→ AbortError,serverStophandleRemainingAccount;completedofkeep.
      if (error instanceof DOMException && error.name === 'AbortError') {
        toast.info('Import stopped (completed accounts are kept)')
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
      case 'skipped':
        return <AlertCircle className="w-5 h-5 text-gray-400" />
      case 'failed':
        return <XCircle className="w-5 h-5 text-red-500" />
    }
  }

  const getStatusText = (result: VerificationResult) => {
    switch (result.status) {
      case 'pending': return 'Waiting'
      case 'checking': return 'Check for duplicates...'
      case 'verifying': return 'Processing...'
      case 'verified': return 'Validation succeeded'
      case 'imported': return 'Imported (not validated)'
      case 'duplicate': return 'Duplicate credential'
      case 'skipped': return 'Skipped (error status)'
      case 'failed':
        if (result.rollbackStatus === 'success') return 'Validation failed (excluded)'
        if (result.rollbackStatus === 'failed') return 'Validation failed (not excluded)'
        return 'Processing failed (not created)'
    }
  }

  // preview the parse result
  const { previewAccounts, parseError } = useMemo(() => {
    if (!jsonInput.trim()) return { previewAccounts: [] as KamAccount[], parseError: '' }
    try {
      return { previewAccounts: parseKamJson(jsonInput), parseError: '' }
    } catch (e) {
      return { previewAccounts: [] as KamAccount[], parseError: extractErrorMessage(e) }
    }
  }, [jsonInput])

  const errorAccountCount = previewAccounts.filter(a => a.status === 'error').length

  // terminated(verified/imported/duplicate/failed/skipped)ofrow count,drive the progressitems
  const finalizedCount = results.filter(
    r =>
      r.status === 'verified' ||
      r.status === 'imported' ||
      r.status === 'duplicate' ||
      r.status === 'failed' ||
      r.status === 'skipped'
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
          <DialogTitle>KAM Account import</DialogTitle>
        </DialogHeader>

        <div className="flex-1 overflow-y-auto space-y-4 py-4">
          <div className="space-y-2">
            <div className="flex items-center justify-between">
              <label className="text-sm font-medium">KAM Export JSON</label>
              <div>
                <input
                  ref={fileInputRef}
                  type="file"
                  accept="application/json,.json"
                  multiple
                  className="hidden"
                  onChange={handleFileSelect}
                />
                <Button
                  type="button"
                  variant="outline"
                  size="sm"
                  onClick={() => fileInputRef.current?.click()}
                  disabled={importing}
                >
                  <Upload className="w-4 h-4 mr-1.5" />
                  Choose file
                </Button>
              </div>
            </div>
            <textarea
              placeholder={'Paste Kiro Account Manager exported JSON, or click the top right“Choose file”Import\n\nSupported KAM 1.8.3+ New flat format:\n[\n  {\n    "email": "...",\n    "refreshToken": "...",\n    "clientId": "...",\n    "clientSecret": "...",\n    "region": "us-east-1"\n  }\n]\n\n(optional authMethod field is ignored; the system will use clientId/clientSecret auto-detect)\n\nThe legacy nested format is also supported:\n{\n  "version": "1.5.0",\n  "accounts": [\n    {\n      "email": "...",\n      "credentials": {\n        "refreshToken": "...",\n        "clientId": "...",\n        "clientSecret": "...",\n        "region": "us-east-1"\n      }\n    }\n  ]\n}'}
              value={jsonInput}
              onChange={(e) => setJsonInput(e.target.value)}
              disabled={importing}
              className="flex min-h-[200px] w-full rounded-xl border border-input bg-background/60 px-3.5 py-2.5 text-sm transition-[border-color,background-color,box-shadow] duration-150 ease-apple placeholder:text-muted-foreground/70 hover:border-border focus-visible:outline-none focus-visible:border-ring focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ring/30 focus-visible:bg-background disabled:cursor-not-allowed disabled:opacity-50 font-mono"
            />
          </div>

          {/* parse preview */}
          {parseError && (
            <div className="text-sm text-red-600 dark:text-red-400">Parse failed: {parseError}</div>
          )}
          {previewAccounts.length > 0 && !importing && results.length === 0 && (
            <div className="space-y-2">
              <div className="text-sm text-muted-foreground">
                Detected {previewAccounts.length} accounts
                {errorAccountCount > 0 && `(of which ${errorAccountCount} items are error status)`}
              </div>
              {errorAccountCount > 0 && (
                <label className="flex items-center gap-2 text-sm">
                  <input
                    type="checkbox"
                    checked={skipErrorAccounts}
                    onChange={(e) => setSkipErrorAccounts(e.target.checked)}
                    className="rounded border-gray-300"
                  />
                  Skip error accounts in status
                </label>
              )}
            </div>
          )}

          {/* Import progressandresult */}
          {(importing || results.length > 0) && (
            <>
              <div className="space-y-2">
                <div className="flex justify-between text-sm">
                  <span>{importing ? 'Import progress' : 'Import complete'}</span>
                  <span>{finalizedCount} / {progress.total}</span>
                </div>
                <div className="w-full bg-secondary rounded-full h-2">
                  <div
                    className="bg-primary h-2 rounded-full transition-all"
                    style={{ width: `${progress.total > 0 ? (finalizedCount / progress.total) * 100 : 0}%` }}
                  />
                </div>
                {importing && currentProcessing && (
                  <div className="text-xs text-muted-foreground">{currentProcessing}</div>
                )}
              </div>

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
                <span className="text-gray-500">
                  ○ Skip: {results.filter(r => r.status === 'skipped').length}
                </span>
              </div>

              <div className="border rounded-md divide-y max-h-[300px] overflow-y-auto">
                {results.map((result) => (
                  <div key={result.index} className="p-3">
                    <div className="flex items-start gap-3">
                      {getStatusIcon(result.status)}
                      <div className="flex-1 min-w-0">
                        <div className="flex items-center gap-2">
                          <span className="text-sm font-medium">
                            {result.email || `Account #${result.index}`}
                          </span>
                          <span className="text-xs text-muted-foreground">
                            {getStatusText(result)}
                          </span>
                        </div>
                        {result.usage && (
                          <div className="text-xs text-muted-foreground mt-1">Usage: {result.usage}</div>
                        )}
                        {result.error && (
                          <div className="text-xs text-red-600 dark:text-red-400 mt-1">{result.error}</div>
                        )}
                        {result.rollbackError && (
                          <div className="text-xs text-red-600 dark:text-red-400 mt-1">Rollback failed: {result.rollbackError}</div>
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
                onClick={() => { onOpenChange(false); resetForm() }}
              >
                {results.length > 0 ? 'Close' : 'Cancel'}
              </Button>
              {results.length === 0 && (
                <>
                  <Button
                    type="button"
                    variant="outline"
                    onClick={() => handleImport(false)}
                    disabled={!jsonInput.trim() || previewAccounts.length === 0 || !!parseError}
                  >
                    Import directly (without validation)
                  </Button>
                  <Button
                    type="button"
                    onClick={() => handleImport(true)}
                    disabled={!jsonInput.trim() || previewAccounts.length === 0 || !!parseError}
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
