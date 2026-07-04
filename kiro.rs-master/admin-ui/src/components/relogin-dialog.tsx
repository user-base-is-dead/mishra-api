import { useState, useEffect, useRef } from 'react'
import { toast } from 'sonner'
import { ExternalLink, Copy, Loader2, CheckCircle } from 'lucide-react'
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogFooter,
} from '@/components/ui/dialog'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import {
  startSocialRelogin,
  pollSocialRelogin,
  completeSocialRelogin,
  startIdcRelogin,
  pollIdcRelogin,
} from '@/api/credentials'
import {
  useUpdateRefreshToken,
  useSetDisabled,
  useResetFailure,
} from '@/hooks/use-credentials'
import type { CredentialStatusItem, StartSocialLoginResponse, StartIdcLoginResponse } from '@/types/api'
import { extractErrorMessage } from '@/lib/utils'
import { useQueryClient } from '@tanstack/react-query'

interface ReloginDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  credential: CredentialStatusItem
}

type Method = 'social' | 'idc' | 'manual'
type Step = 'select' | 'form' | 'waiting' | 'manual-updating' | 'done'

const POLL_INTERVAL_MS = 2000

const isRemoteAccess = () =>
  window.location.hostname !== 'localhost' && window.location.hostname !== '127.0.0.1'

function parseCallbackUrl(rawUrl: string): { code: string; state: string; loginOption: string; path: string } | null {
  try {
    const url = new URL(rawUrl.trim())
    const code = url.searchParams.get('code')
    const state = url.searchParams.get('state')
    if (!code || !state) return null
    return {
      code,
      state,
      loginOption: url.searchParams.get('login_option') ?? '',
      path: url.pathname,
    }
  } catch {
    return null
  }
}

interface ParsedTokenData {
  refreshToken: string
  email?: string
}

function parseTokenInput(input: string): ParsedTokenData {
  const trimmed = input.trim()
  if (!trimmed) return { refreshToken: '' }

  try {
    const parsed = JSON.parse(trimmed)
    const extractFromObj = (obj: Record<string, unknown>): ParsedTokenData | null => {
      const rt = typeof obj.refreshToken === 'string' ? obj.refreshToken.trim() : ''
      if (!rt) return null
      const email = typeof obj.email === 'string' ? obj.email.trim() : undefined
      return { refreshToken: rt, email: email || undefined }
    }

    const direct = extractFromObj(parsed as Record<string, unknown>)
    if (direct) return direct

    if (parsed.credentials) {
      const nested = extractFromObj(parsed.credentials as Record<string, unknown>)
      if (nested) {
        const outerEmail = typeof (parsed as Record<string, unknown>).email === 'string'
          ? ((parsed as Record<string, unknown>).email as string).trim() || undefined
          : undefined
        return { ...nested, email: nested.email ?? outerEmail }
      }
    }

    if (Array.isArray(parsed) && parsed.length > 0) {
      const first = parsed[0] as Record<string, unknown>
      const fromFirst = extractFromObj(first)
      if (fromFirst) return fromFirst
    }

    return { refreshToken: '' }
  } catch {
    return { refreshToken: trimmed }
  }
}

export function ReloginDialog({ open, onOpenChange, credential }: ReloginDialogProps) {
  const [method, setMethod] = useState<Method>('social')
  const [step, setStep] = useState<Step>('select')

  // Social/IdC form field
  const [isStarting, setIsStarting] = useState(false)
  const [isCompleting, setIsCompleting] = useState(false)
  const [callbackUrl, setCallbackUrl] = useState('')
  const [socialSession, setSocialSession] = useState<StartSocialLoginResponse | null>(null)
  const [idcSession, setIdcSession] = useState<StartIdcLoginResponse | null>(null)
  // IdC form
  const [idcRegion, setIdcRegion] = useState('us-east-1')
  const [idcStartUrl, setIdcStartUrl] = useState('')

  // Manual field
  const [manualInput, setManualInput] = useState('')
  const [manualLog, setManualLog] = useState<string[]>([])

  const pollTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null)
  const isRemote = isRemoteAccess()

  const queryClient = useQueryClient()
  const updateRefreshToken = useUpdateRefreshToken()
  const setDisabled = useSetDisabled()
  const resetFailure = useResetFailure()

  useEffect(() => {
    return () => {
      if (pollTimerRef.current) clearTimeout(pollTimerRef.current)
    }
  }, [])

  const invalidate = () => queryClient.invalidateQueries({ queryKey: ['credentials'] })

  const handleClose = () => {
    if (pollTimerRef.current) clearTimeout(pollTimerRef.current)
    setStep('select')
    setSocialSession(null)
    setIdcSession(null)
    setIsStarting(false)
    setIsCompleting(false)
    setCallbackUrl('')
    setManualInput('')
    setManualLog([])
    onOpenChange(false)
  }

  // ─── Social ───────────────────────────────────────────────────────────────

  const handleStartSocial = async () => {
    setIsStarting(true)
    // must be within await open the window synchronously beforehand,otherwise the browser popup blocker causesjumpconvert to currentpage
    const loginWindow = window.open('about:blank', '_blank')
    try {
      const resp = await startSocialRelogin(credential.id, {})
      setSocialSession(resp)
      setStep('waiting')
      if (loginWindow) {
        loginWindow.location.href = resp.portalUrl
      } else {
        window.open(resp.portalUrl, '_blank')
      }
      // always poll:server remote mode(resp.remote)bypublic callback pathbyauto complete,local modebylocal callback complete.
      scheduleSocialPoll(resp.sessionId)
    } catch (e) {
      loginWindow?.close()
      toast.error('Start login failed:' + extractErrorMessage(e))
    } finally {
      setIsStarting(false)
    }
  }

  const scheduleSocialPoll = (sessionId: string) => {
    pollTimerRef.current = setTimeout(async () => {
      try {
        const result = await pollSocialRelogin(credential.id, sessionId)
        if (result.status === 'pending') {
          scheduleSocialPoll(sessionId)
        } else if (result.status === 'success') {
          setStep('done')
          invalidate()
          toast.success(`Credential #${result.credentialId} Token Updated and enabled`)
        } else {
          toast.error('Session expired. Please start login again')
          setStep('form')
          setSocialSession(null)
        }
      } catch (e) {
        toast.error('Round-robin failed:' + extractErrorMessage(e))
        scheduleSocialPoll(sessionId)
      }
    }, POLL_INTERVAL_MS)
  }

  const handleCompleteSocialManually = async () => {
    if (!socialSession) return
    const parsed = parseCallbackUrl(callbackUrl)
    if (!parsed) {
      toast.error('URL Invalid format. Please copy the full address bar URL')
      return
    }
    setIsCompleting(true)
    try {
      const result = await completeSocialRelogin(credential.id, socialSession.sessionId, {
        code: parsed.code,
        state: parsed.state,
        loginOption: parsed.loginOption || undefined,
        path: parsed.path,
      })
      if (result.status === 'success') {
        setStep('done')
        invalidate()
        toast.success(`Credential #${result.credentialId} Token Updated and enabled`)
      } else {
        toast.error('Session expired. Please start login again')
        setStep('form')
        setSocialSession(null)
      }
    } catch (e) {
      toast.error('Complete login failed:' + extractErrorMessage(e))
    } finally {
      setIsCompleting(false)
    }
  }

  // ─── IdC ──────────────────────────────────────────────────────────────────

  const handleStartIdc = async () => {
    if (!idcRegion.trim()) {
      toast.error('Please fill in AWS Region')
      return
    }
    setIsStarting(true)
    try {
      const resp = await startIdcRelogin(credential.id, {
        region: idcRegion.trim(),
        startUrl: idcStartUrl.trim() || undefined,
      })
      setIdcSession(resp)
      setStep('waiting')
      scheduleIdcPoll(resp.sessionId, resp.pollInterval)
    } catch (e) {
      toast.error('Start login failed:' + extractErrorMessage(e))
    } finally {
      setIsStarting(false)
    }
  }

  const scheduleIdcPoll = (sessionId: string, interval: number) => {
    pollTimerRef.current = setTimeout(async () => {
      try {
        const result = await pollIdcRelogin(credential.id, sessionId)
        if (result.status === 'pending') {
          scheduleIdcPoll(sessionId, interval)
        } else if (result.status === 'success') {
          setStep('done')
          invalidate()
          toast.success(`Credential #${result.credentialId} Token Updated and enabled`)
        } else {
          toast.error('Authorization expired. Please start login again')
          setStep('form')
          setIdcSession(null)
        }
      } catch (e) {
        toast.error('Round-robin failed:' + extractErrorMessage(e))
        scheduleIdcPoll(sessionId, interval)
      }
    }, interval * 1000)
  }

  const copyIdcCode = () => {
    if (!idcSession) return
    navigator.clipboard.writeText(idcSession.userCode)
    toast.success('Verification code copied')
  }

  // ─── Manual ───────────────────────────────────────────────────────────────

  const parsed = parseTokenInput(manualInput)
  const extractedToken = parsed.refreshToken
  const isManualValid = extractedToken.length >= 100 && !extractedToken.includes('...')
  const isManualUpdating = step === 'manual-updating'

  const addLog = (msg: string) => setManualLog(prev => [...prev, msg])

  const handleManualSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    if (!isManualValid) {
      toast.error('refreshToken Invalid or truncated')
      return
    }
    setStep('manual-updating')
    setManualLog([])

    try {
      if (!credential.disabled) {
        addLog('Temporarily disabling the credential…')
        await new Promise<void>((resolve, reject) => {
          setDisabled.mutate({ id: credential.id, disabled: true }, { onSuccess: () => resolve(), onError: reject })
        })
        addLog('✓ Temporarily disabled')
      }

      addLog('Updating refreshToken…')
      await new Promise<void>((resolve, reject) => {
        updateRefreshToken.mutate(
          { id: credential.id, req: { refreshToken: extractedToken } },
          { onSuccess: () => resolve(), onError: reject }
        )
      })
      addLog('✓ refreshToken Updated')

      addLog('Resetting the failure count and enabling…')
      await new Promise<void>((resolve, reject) => {
        resetFailure.mutate(credential.id, { onSuccess: () => resolve(), onError: reject })
      })
      addLog('✓ Reset and enabled')

      setStep('done')
      invalidate()
      toast.success(`Credential #${credential.id} Reimport complete, automatically enabled`)
    } catch (error) {
      addLog(`✗ Failed: ${extractErrorMessage(error)}`)
      setStep('select')
      toast.error(`Operation failed: ${extractErrorMessage(error)}`)
    }
  }

  // ─── Render ───────────────────────────────────────────────────────────────

  const displayName = credential.email || `Credential #${credential.id}`
  const authMethod = credential.authMethod

  return (
    <Dialog open={open} onOpenChange={handleClose}>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>Log in again — {displayName}</DialogTitle>
          <DialogDescription>
            Choose a login method; once done the credential will be refreshed Token and re-enable automatically.
          </DialogDescription>
        </DialogHeader>

        {/* method selection */}
        {step === 'select' && (
          <div className="space-y-3 py-2">
            <p className="text-sm text-muted-foreground">Choose a re-login method:</p>
            <div className="grid gap-2">
              <button
                onClick={() => { setMethod('social'); setStep('form') }}
                className={`flex items-start gap-3 rounded-lg border p-3 text-left transition-colors hover:bg-accent ${authMethod === 'social' ? 'border-primary bg-accent/50' : ''}`}
              >
                <div>
                  <p className="text-sm font-medium">Social Login (Google / GitHub)</p>
                  <p className="text-xs text-muted-foreground mt-0.5">via Kiro Complete on the web OAuth Authorize</p>
                </div>
              </button>
              <button
                onClick={() => { setMethod('idc'); setStep('form') }}
                className={`flex items-start gap-3 rounded-lg border p-3 text-left transition-colors hover:bg-accent ${authMethod === 'idc' ? 'border-primary bg-accent/50' : ''}`}
              >
                <div>
                  <p className="text-sm font-medium">AWS SSO / Builder ID(IdC)</p>
                  <p className="text-xs text-muted-foreground mt-0.5">via AWS Identity Center Device authorization</p>
                </div>
              </button>
              <button
                onClick={() => { setMethod('manual'); setStep('form') }}
                className="flex items-start gap-3 rounded-lg border p-3 text-left transition-colors hover:bg-accent"
              >
                <div>
                  <p className="text-sm font-medium">Paste manually Token</p>
                  <p className="text-xs text-muted-foreground mt-0.5">Paste KAM JSON or refreshToken string</p>
                </div>
              </button>
            </div>
          </div>
        )}

        {/* Social form */}
        {step === 'form' && method === 'social' && (
          <div className="py-2 space-y-3">
            <p className="text-sm text-muted-foreground">
              Click Start login and the browser will open Kiro login page. After authorization,Token will be updated to this credential automatically.
            </p>
          </div>
        )}

        {/* Social etc.pending */}
        {step === 'waiting' && method === 'social' && socialSession && (
          <div className="space-y-4 py-2">
            <div className="rounded-lg border bg-muted/50 p-4 space-y-3">
              <p className="text-sm text-muted-foreground">The browser should have opened automatically Kiro login page. Please complete authorization.</p>
              <a
                href={socialSession.portalUrl}
                target="_blank"
                rel="noopener noreferrer"
                className="inline-flex items-center gap-1.5 text-sm font-medium text-primary hover:underline"
              >
                Reopen the login page
                <ExternalLink className="h-3.5 w-3.5" />
              </a>
            </div>
            {isRemote && !socialSession.remote ? (
              // remote browser access and the server has notConfig callbackBaseUrl:Paste manuallyfallback
              <div className="space-y-2">
                <p className="text-sm text-amber-600 dark:text-amber-400">
                  After logging in, copy the full URL from the address bar URL Paste below:
                </p>
                <textarea
                  placeholder="http://localhost:3128/oauth/callback?code=...&state=...&login_option=google"
                  value={callbackUrl}
                  onChange={(e) => setCallbackUrl(e.target.value)}
                  disabled={isCompleting}
                  className="flex min-h-[80px] w-full rounded-md border border-input bg-background px-3 py-2 text-xs font-mono placeholder:text-muted-foreground focus-visible:outline-none focus-visible:border-ring focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ring/30 disabled:opacity-50"
                />
              </div>
            ) : (
              <div className="flex items-center gap-2 text-sm text-muted-foreground">
                <Loader2 className="h-4 w-4 animate-spin" />
                {socialSession.remote
                  ? 'After login the browser returns to this service automatically. Waiting for automatic completion…'
                  : 'Waiting for login to complete…'}
              </div>
            )}
          </div>
        )}

        {/* IdC form */}
        {step === 'form' && method === 'idc' && (
          <div className="space-y-3 py-2">
            <div className="space-y-1.5">
              <label className="text-sm font-medium">AWS Region</label>
              <Input
                placeholder="us-east-1"
                value={idcRegion}
                onChange={(e) => setIdcRegion(e.target.value)}
              />
            </div>
            <div className="space-y-1.5">
              <label className="text-sm font-medium">
                SSO Start URL
                <span className="ml-1 text-xs text-muted-foreground">(leave empty to use AWS Builder ID)</span>
              </label>
              <Input
                placeholder="https://view.awsapps.com/start"
                value={idcStartUrl}
                onChange={(e) => setIdcStartUrl(e.target.value)}
              />
            </div>
          </div>
        )}

        {/* IdC etc.pending */}
        {step === 'waiting' && method === 'idc' && idcSession && (
          <div className="space-y-4 py-2">
            <div className="rounded-lg border bg-muted/50 p-4 text-center space-y-3">
              <p className="text-sm text-muted-foreground">Open the following address in your browser and enter the verification code</p>
              <a
                href={idcSession.verificationUriComplete ?? idcSession.verificationUri}
                target="_blank"
                rel="noopener noreferrer"
                className="inline-flex items-center gap-1.5 text-sm font-medium text-primary hover:underline"
              >
                {idcSession.verificationUri}
                <ExternalLink className="h-3.5 w-3.5" />
              </a>
              <div className="flex items-center justify-center gap-2">
                <span className="font-mono text-2xl font-bold tracking-widest">{idcSession.userCode}</span>
                <Button variant="ghost" size="icon" className="h-7 w-7" onClick={copyIdcCode}>
                  <Copy className="h-3.5 w-3.5" />
                </Button>
              </div>
            </div>
            <div className="flex items-center gap-2 text-sm text-muted-foreground">
              <Loader2 className="h-4 w-4 animate-spin" />
              Waiting for authorization. Please complete the login in your browser…
            </div>
          </div>
        )}

        {/* Manual form */}
        {(step === 'form' || step === 'manual-updating') && method === 'manual' && (
          <form onSubmit={handleManualSubmit}>
            <div className="space-y-3 py-2">
              <label className="text-sm font-medium">
                Paste KAM Export JSON or refreshToken string
              </label>
              <textarea
                placeholder={'The following formats are supported:\n\n1. Paste directly refreshToken string\n\n2. KAM Export JSON:\n{\n  "email": "...",\n  "refreshToken": "aor..."\n}'}
                value={manualInput}
                onChange={(e) => setManualInput(e.target.value)}
                disabled={isManualUpdating}
                className="flex min-h-[140px] w-full rounded-xl border border-input bg-background/60 px-3.5 py-2.5 text-sm transition-[border-color,background-color,box-shadow] duration-150 ease-apple placeholder:text-muted-foreground/70 hover:border-border focus-visible:outline-none focus-visible:border-ring focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ring/30 focus-visible:bg-background disabled:opacity-50 font-mono"
              />
              {manualInput.trim() && step === 'form' && (
                <div className={`text-sm rounded-md p-3 ${isManualValid ? 'bg-green-50 dark:bg-green-950 text-green-700 dark:text-green-300' : 'bg-red-50 dark:bg-red-950 text-red-700 dark:text-red-300'}`}>
                  {isManualValid ? (
                    <>
                      Recognized refreshToken({extractedToken.length} characters):
                      <span className="font-mono text-xs block mt-1 opacity-75">
                        {extractedToken.slice(0, 20)}...{extractedToken.slice(-10)}
                      </span>
                    </>
                  ) : (
                    extractedToken.length > 0
                      ? `Token Invalid: length ${extractedToken.length} characters (need ≥100 characters)`
                      : 'Unrecognized refreshToken, please check the format'
                  )}
                </div>
              )}
              {manualLog.length > 0 && (
                <div className="rounded-md border bg-muted/40 p-3 space-y-1">
                  {manualLog.map((log, i) => (
                    <div key={i} className="text-sm font-mono">{log}</div>
                  ))}
                </div>
              )}
            </div>
          </form>
        )}

        {/* complete */}
        {step === 'done' && (
          <div className="flex flex-col items-center gap-3 py-4">
            <CheckCircle className="h-10 w-10 text-green-500" />
            <p className="text-sm font-medium">Token Updated, credential enabled</p>
            <p className="text-xs text-muted-foreground">{displayName}</p>
          </div>
        )}

        <DialogFooter>
          {step === 'select' && (
            <Button variant="outline" onClick={handleClose}>Cancel</Button>
          )}

          {step === 'form' && method === 'social' && (
            <>
              <Button variant="outline" onClick={() => setStep('select')}>Back</Button>
              <Button onClick={handleStartSocial} disabled={isStarting}>
                {isStarting && <Loader2 className="mr-2 h-4 w-4 animate-spin" />}
                Start login
              </Button>
            </>
          )}

          {step === 'waiting' && method === 'social' && (
            <>
              <Button variant="outline" onClick={handleClose} disabled={isCompleting}>Cancel</Button>
              {isRemote && socialSession && !socialSession.remote && (
                <Button
                  onClick={handleCompleteSocialManually}
                  disabled={isCompleting || !callbackUrl.trim()}
                >
                  {isCompleting && <Loader2 className="mr-2 h-4 w-4 animate-spin" />}
                  Complete login
                </Button>
              )}
            </>
          )}

          {step === 'form' && method === 'idc' && (
            <>
              <Button variant="outline" onClick={() => setStep('select')}>Back</Button>
              <Button onClick={handleStartIdc} disabled={isStarting}>
                {isStarting && <Loader2 className="mr-2 h-4 w-4 animate-spin" />}
                Start login
              </Button>
            </>
          )}

          {step === 'waiting' && method === 'idc' && (
            <Button variant="outline" onClick={handleClose}>Cancel</Button>
          )}

          {(step === 'form' || step === 'manual-updating') && method === 'manual' && (
            <>
              <Button variant="outline" onClick={() => setStep('select')} disabled={isManualUpdating}>Back</Button>
              <Button
                onClick={(e) => handleManualSubmit(e as unknown as React.FormEvent)}
                disabled={isManualUpdating || !isManualValid}
              >
                {isManualUpdating ? <Loader2 className="mr-2 h-4 w-4 animate-spin" /> : null}
                {isManualUpdating ? 'Processing…' : 'Confirm update'}
              </Button>
            </>
          )}

          {step === 'done' && (
            <Button onClick={handleClose}>Close</Button>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
