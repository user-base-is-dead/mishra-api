import { useState, useEffect, useRef } from 'react'
import { toast } from 'sonner'
import { ExternalLink, CheckCircle, Loader2, Copy, Check } from 'lucide-react'
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
import { startSocialLogin, pollSocialLogin, completeSocialLogin } from '@/api/credentials'
import type { StartSocialLoginResponse } from '@/types/api'
import { extractErrorMessage } from '@/lib/utils'

interface SocialLoginDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  onSuccess: () => void
}

type Step = 'form' | 'waiting' | 'done'
type CopyState = 'idle' | 'copied' | 'manual'

const POLL_INTERVAL_MS = 2000

// check whetherasremote access(notLocal)
const isRemoteAccess = () =>
  window.location.hostname !== 'localhost' && window.location.hostname !== '127.0.0.1'

// fromcallback URL stringextract from OAuth parameter
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

async function getClipboardWritePermission(): Promise<PermissionState | 'unsupported'> {
  if (!navigator.permissions?.query) return 'unsupported'
  try {
    const status = await navigator.permissions.query({ name: 'clipboard-write' as PermissionName })
    return status.state
  } catch {
    return 'unsupported'
  }
}

export function SocialLoginDialog({ open, onOpenChange, onSuccess }: SocialLoginDialogProps) {
  const [step, setStep] = useState<Step>('form')
  const [email, setEmail] = useState('')
  const [incognito, setIncognito] = useState(false)
  const [copyState, setCopyState] = useState<CopyState>('idle')
  const [isStarting, setIsStarting] = useState(false)
  const [isCompleting, setIsCompleting] = useState(false)
  const [session, setSession] = useState<StartSocialLoginResponse | null>(null)
  const [credentialId, setCredentialId] = useState<number | null>(null)
  const [callbackUrl, setCallbackUrl] = useState('')
  const pollTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null)
  const loginLinkRef = useRef<HTMLTextAreaElement | null>(null)
  const isRemote = isRemoteAccess()

  useEffect(() => {
    return () => {
      if (pollTimerRef.current) clearTimeout(pollTimerRef.current)
    }
  }, [])

  const handleOpenChange = (v: boolean) => {
    if (!v) {
      if (pollTimerRef.current) clearTimeout(pollTimerRef.current)
      setStep('form')
      setSession(null)
      setCredentialId(null)
      setIsStarting(false)
      setIsCompleting(false)
      setCallbackUrl('')
      setCopyState('idle')
    }
    onOpenChange(v)
  }

  const selectLoginLink = () => {
    window.requestAnimationFrame(() => {
      loginLinkRef.current?.focus()
      loginLinkRef.current?.select()
    })
  }

  const handleCopyLink = async (url: string) => {
    if (!window.isSecureContext || !navigator.clipboard?.writeText) {
      setCopyState('manual')
      selectLoginLink()
      toast.error('The current address is not HTTPS/localhost, the browser does not allow pages to write to the clipboard directly')
      return
    }

    try {
      const permission = await getClipboardWritePermission()
      if (permission === 'denied') {
        setCopyState('manual')
        selectLoginLink()
        toast.error('Chrome Clipboard write was denied for this site. Allow it in the address bar permission settings and retry')
        return
      }

      await navigator.clipboard.writeText(url)
      setCopyState('copied')
      toast.success('Login link copied. Paste and open it in an incognito window')
      setTimeout(() => setCopyState('idle'), 2000)
    } catch {
      setCopyState('manual')
      selectLoginLink()
      toast.error('The browser denied clipboard write. The link is selected, please press Ctrl+C Copy')
    }
  }

  const handleStart = async () => {
    setIsStarting(true)
    // Incognitomode:the browser does not allow JS open directlyIncognitowindow,changeasCopylink lets the user do it manuallyinIncognitowindow opens,
    // therefore do not preopen about:blank(avoidincurrentLoginwrongly opened in this state).
    const loginWindow = incognito ? null : window.open('about:blank', '_blank')
    try {
      const resp = await startSocialLogin({
        email: email.trim() || undefined,
      })
      setSession(resp)
      setStep('waiting')
      if (incognito) {
        // Incognito:priorityWriteclipboard;when rejected by browser policy the link below is selected.
        await handleCopyLink(resp.portalUrl)
      } else if (loginWindow) {
        loginWindow.location.href = resp.portalUrl
      } else {
        window.open(resp.portalUrl, '_blank')
      }
      // always poll:local modebylocal callback server delivery;server remote mode(resp.remote)
      // bypublic network GET callback pathbydeliver channel,polling can complete it automatically.
      // only remote browser access and notConfig callbackBaseUrl toward the bottom at that timePaste manuallyfallback(polling is harmless).
      schedulePoll(resp.sessionId)
    } catch (e) {
      loginWindow?.close()
      toast.error('Start login failed:' + extractErrorMessage(e))
    } finally {
      setIsStarting(false)
    }
  }

  const schedulePoll = (sessionId: string) => {
    pollTimerRef.current = setTimeout(async () => {
      try {
        const result = await pollSocialLogin(sessionId)
        if (result.status === 'pending') {
          schedulePoll(sessionId)
        } else if (result.status === 'success') {
          setCredentialId(result.credentialId)
          setStep('done')
          onSuccess()
          toast.success(`Login succeeded, credential added #${result.credentialId}`)
        } else {
          toast.error('Session expired. Please start login again')
          setStep('form')
          setSession(null)
        }
      } catch (e) {
        toast.error('Round-robin failed:' + extractErrorMessage(e))
        schedulePoll(sessionId)
      }
    }, POLL_INTERVAL_MS)
  }

  const handleCompleteManually = async () => {
    if (!session) return
    const parsed = parseCallbackUrl(callbackUrl)
    if (!parsed) {
      toast.error('URL Invalid format. Please copy the full address bar URL')
      return
    }
    setIsCompleting(true)
    try {
      const result = await completeSocialLogin(session.sessionId, {
        code: parsed.code,
        state: parsed.state,
        loginOption: parsed.loginOption || undefined,
        path: parsed.path,
      })
      if (result.status === 'success') {
        setCredentialId(result.credentialId)
        setStep('done')
        onSuccess()
        toast.success(`Login succeeded, credential added #${result.credentialId}`)
      } else {
        toast.error('Session expired. Please start login again')
        setStep('form')
        setSession(null)
      }
    } catch (e) {
      toast.error('Complete login failed:' + extractErrorMessage(e))
    } finally {
      setIsCompleting(false)
    }
  }

  return (
    <Dialog open={open} onOpenChange={handleOpenChange}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>Kiro Account login (Google / GitHub)</DialogTitle>
          <DialogDescription>
            via Kiro Complete on the web Social log in, no manual export needed refreshToken.
          </DialogDescription>
        </DialogHeader>

        {step === 'form' && (
          <div className="space-y-4 py-2">
            <div className="space-y-1.5">
              <label htmlFor="social-email" className="text-sm font-medium">Email (optional)</label>
              <Input
                id="social-email"
                placeholder="user@example.com"
                value={email}
                onChange={(e) => setEmail(e.target.value)}
              />
            </div>
            <label className="flex items-start gap-2 rounded-lg border bg-muted/40 p-3 cursor-pointer">
              <input
                type="checkbox"
                checked={incognito}
                onChange={(e) => setIncognito(e.target.checked)}
                className="mt-0.5 h-4 w-4 shrink-0 accent-primary"
              />
              <span className="text-sm">
                <span className="font-medium">Log in using an incognito window</span>
                <span className="mt-0.5 block text-xs text-muted-foreground">
                  After starting, copy the login link and use an incognito browser yourself/Incognito window (Ctrl+Shift+N) to open,
                  to avoid conflicting with the currently logged-in Google / GitHub account mix-up.
                </span>
              </span>
            </label>
          </div>
        )}

        {step === 'waiting' && session && (
          <div className="space-y-4 py-2">
            {incognito ? (
              <div className="rounded-lg border bg-muted/50 p-4 space-y-3">
                <p className="text-sm text-muted-foreground">
                  {copyState === 'copied'
                    ? 'Login link copied.'
                    : 'After copying the login link,'}
                  Please open a new<span className="font-medium text-foreground">Incognito / Incognito window</span>
                  (Ctrl+Shift+N,Safari as ⌘+Shift+N), paste to open and complete authorization.
                </p>
                <textarea
                  ref={loginLinkRef}
                  readOnly
                  value={session.portalUrl}
                  onFocus={(e) => e.currentTarget.select()}
                  className="flex min-h-[72px] w-full resize-none rounded-md border border-input bg-background px-3 py-2 text-xs font-mono text-muted-foreground focus-visible:outline-none focus-visible:border-ring focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ring/30"
                />
                <div className="flex items-center gap-2">
                  <Button
                    size="sm"
                    variant="outline"
                    onClick={() => handleCopyLink(session.portalUrl)}
                  >
                    {copyState === 'copied' ? (
                      <Check className="h-3.5 w-3.5" />
                    ) : (
                      <Copy className="h-3.5 w-3.5" />
                    )}
                    {copyState === 'copied' ? 'Copied' : 'Copy login link'}
                  </Button>
                  {copyState === 'manual' && (
                    <span className="text-xs text-muted-foreground">
                      The link is selected. You can directly press Ctrl+C
                    </span>
                  )}
                </div>
              </div>
            ) : (
              <div className="rounded-lg border bg-muted/50 p-4 space-y-3">
                <p className="text-sm text-muted-foreground">
                  The browser should have opened automatically Kiro login page. Please complete authorization.
                </p>
                <a
                  href={session.portalUrl}
                  target="_blank"
                  rel="noopener noreferrer"
                  className="inline-flex items-center gap-1.5 text-sm font-medium text-primary hover:underline"
                >
                  Reopen the login page
                  <ExternalLink className="h-3.5 w-3.5" />
                </a>
              </div>
            )}

            {isRemote && !session.remote ? (
              // remote browser access and the server has notConfig callbackBaseUrl:OAuth callback to localhost cannot be captured,
              // requires the userfromaddress barCopyfull URL Paste manuallycomplete.
              <div className="space-y-2">
                <p className="text-sm text-amber-600 dark:text-amber-400">
                  After logging in, the browser will redirect to <code>localhost</code> failure page,
                  Please copy the full URL from the address bar URL Paste below:
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
                {session.remote
                  ? 'After login the browser returns to this service automatically. Waiting for automatic completion…'
                  : 'Waiting for login to complete…'}
              </div>
            )}
          </div>
        )}

        {step === 'done' && (
          <div className="flex flex-col items-center gap-3 py-4">
            <CheckCircle className="h-10 w-10 text-green-500" />
            <p className="text-sm font-medium">Login succeeded</p>
            <p className="text-xs text-muted-foreground">Credential #{credentialId} Added and enabled</p>
          </div>
        )}

        <DialogFooter>
          {step === 'form' && (
            <Button onClick={handleStart} disabled={isStarting}>
              {isStarting && <Loader2 className="mr-2 h-4 w-4 animate-spin" />}
              Start login
            </Button>
          )}
          {step === 'waiting' && (
            <>
              <Button variant="outline" onClick={() => handleOpenChange(false)} disabled={isCompleting}>
                Cancel
              </Button>
              {isRemote && session && !session.remote && (
                <Button
                  onClick={handleCompleteManually}
                  disabled={isCompleting || !callbackUrl.trim()}
                >
                  {isCompleting && <Loader2 className="mr-2 h-4 w-4 animate-spin" />}
                  Complete login
                </Button>
              )}
            </>
          )}
          {step === 'done' && (
            <Button onClick={() => handleOpenChange(false)}>Close</Button>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
