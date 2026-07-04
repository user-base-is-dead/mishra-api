import { useState } from 'react'
import { toast } from 'sonner'
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
  DialogDescription,
} from '@/components/ui/dialog'
import { Button } from '@/components/ui/button'
import { useUpdateRefreshToken, useSetDisabled, useResetFailure, useUpdateCredential } from '@/hooks/use-credentials'
import { extractErrorMessage } from '@/lib/utils'
import type { CredentialStatusItem } from '@/types/api'

interface UpdateTokenDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  credential: CredentialStatusItem
}

interface ParsedTokenData {
  refreshToken: string
  email?: string
  accessToken?: string
  expiresAt?: string
}

// from KAM JSON orplainstringextract from token related field
function parseTokenInput(input: string): ParsedTokenData {
  const trimmed = input.trim()
  if (!trimmed) return { refreshToken: '' }

  try {
    const parsed = JSON.parse(trimmed)

    const extractFromObj = (obj: Record<string, unknown>): ParsedTokenData | null => {
      const rt = typeof obj.refreshToken === 'string' ? obj.refreshToken.trim() : ''
      if (!rt) return null
      const email = typeof obj.email === 'string' ? obj.email.trim() : undefined
      const accessToken = typeof obj.accessToken === 'string' ? obj.accessToken.trim() : undefined
      const expiresAt = typeof obj.expiresAt === 'string' ? obj.expiresAt.trim() : undefined
      return {
        refreshToken: rt,
        email: email || undefined,
        accessToken: accessToken || undefined,
        expiresAt: expiresAt || undefined,
      }
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
      if (first.credentials) {
        const nested = extractFromObj(first.credentials as Record<string, unknown>)
        if (nested) {
          const outerEmail = typeof first.email === 'string'
            ? (first.email as string).trim() || undefined
            : undefined
          return { ...nested, email: nested.email ?? outerEmail }
        }
      }
    }

    return { refreshToken: '' }
  } catch {
    return { refreshToken: trimmed }
  }
}

export function UpdateTokenDialog({ open, onOpenChange, credential }: UpdateTokenDialogProps) {
  const [input, setInput] = useState('')
  const [step, setStep] = useState<'idle' | 'updating' | 'done'>('idle')
  const [stepLog, setStepLog] = useState<string[]>([])

  const updateRefreshToken = useUpdateRefreshToken()
  const updateCredential = useUpdateCredential()
  const setDisabled = useSetDisabled()
  const resetFailure = useResetFailure()

  const parsed = parseTokenInput(input)
  const extractedToken = parsed.refreshToken
  const extractedEmail = parsed.email
  const isValid = extractedToken.length >= 100 && !extractedToken.includes('...')
  const isPending = step === 'updating'

  const addLog = (msg: string) => setStepLog(prev => [...prev, msg])

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    if (!isValid) {
      toast.error('refreshToken Invalid or truncated')
      return
    }

    setStep('updating')
    setStepLog([])

    try {
      // step 1:ifCredentialnotDisable,firstDisable(the backend requires an update Token must beforeDisable)
      if (!credential.disabled) {
        addLog('Temporarily disabling the credential…')
        await new Promise<void>((resolve, reject) => {
          setDisabled.mutate(
            { id: credential.id, disabled: true },
            { onSuccess: () => resolve(), onError: reject }
          )
        })
        addLog('✓ Temporarily disabled')
      }

      // step 2:update refreshToken(if JSON contains accessToken then keep it too,avoid calling the auth server immediately)
      addLog('Updating refreshToken…')
      await new Promise<void>((resolve, reject) => {
        updateRefreshToken.mutate(
          {
            id: credential.id,
            req: {
              refreshToken: extractedToken,
              accessToken: parsed.accessToken,
              expiresAt: parsed.expiresAt,
            },
          },
          { onSuccess: () => resolve(), onError: reject }
        )
      })
      addLog(`✓ refreshToken Updated${parsed.accessToken ? '(including accessToken)' : ''}`)

      // step 3:Reset failure count
      addLog('Resetting the failure count…')
      await new Promise<void>((resolve, reject) => {
        resetFailure.mutate(credential.id, {
          onSuccess: () => resolve(),
          onError: reject,
        })
      })
      addLog('✓ Failure count reset')

      // step 4:EnableCredential
      addLog('Re-enabling the credential…')
      await new Promise<void>((resolve, reject) => {
        setDisabled.mutate(
          { id: credential.id, disabled: false },
          { onSuccess: () => resolve(), onError: reject }
        )
      })
      addLog('✓ Credential enabled')

      // step 5:if JSON contains email and differs from the current,sync update
      if (extractedEmail && extractedEmail !== credential.email) {
        addLog(`Updating email to ${extractedEmail}…`)
        await new Promise<void>((resolve, reject) => {
          updateCredential.mutate(
            { id: credential.id, req: { email: extractedEmail } },
            { onSuccess: () => resolve(), onError: reject }
          )
        })
        addLog(`✓ Email updated to ${extractedEmail}`)
      }

      setStep('done')
      toast.success(`Credential #${credential.id} Reimport complete, automatically enabled`)
    } catch (error) {
      addLog(`✗ Failed: ${extractErrorMessage(error)}`)
      setStep('idle')
      toast.error(`Reimport failed: ${extractErrorMessage(error)}`)
    }
  }

  const handleClose = () => {
    if (isPending) return
    setInput('')
    setStep('idle')
    setStepLog([])
    onOpenChange(false)
  }

  return (
    <Dialog open={open} onOpenChange={handleClose}>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>Reimport credentials #{credential.id}</DialogTitle>
          <DialogDescription>
            as {credential.email || `Credential #${credential.id}`} Paste the new Token,
            The system will update automatically Token, reset the failure count, and re-enable.
          </DialogDescription>
        </DialogHeader>

        <form onSubmit={handleSubmit}>
          <div className="space-y-4 py-4">
            <div className="space-y-2">
              <label className="text-sm font-medium">
                Paste KAM Export JSON or paste directly refreshToken string
              </label>
              <textarea
                placeholder={'The following formats are supported:\n\n1. Paste directly refreshToken string\n\n2. KAM Exported single account JSON:\n{\n  "email": "...",\n  "refreshToken": "aor...",\n  "authMethod": "social"\n}'}
                value={input}
                onChange={(e) => setInput(e.target.value)}
                disabled={isPending || step === 'done'}
                className="flex min-h-[140px] w-full rounded-xl border border-input bg-background/60 px-3.5 py-2.5 text-sm transition-[border-color,background-color,box-shadow] duration-150 ease-apple placeholder:text-muted-foreground/70 hover:border-border focus-visible:outline-none focus-visible:border-ring focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ring/30 focus-visible:bg-background disabled:cursor-not-allowed disabled:opacity-50 font-mono"
              />
            </div>

            {/* Token parse preview */}
            {input.trim() && step === 'idle' && (
              <div className={`text-sm rounded-md p-3 ${isValid ? 'bg-green-50 dark:bg-green-950 text-green-700 dark:text-green-300' : 'bg-red-50 dark:bg-red-950 text-red-700 dark:text-red-300'}`}>
                {isValid ? (
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

            {/* execution stepLogs */}
            {stepLog.length > 0 && (
              <div className="rounded-md border bg-muted/40 p-3 space-y-1">
                {stepLog.map((log, i) => (
                  <div key={i} className="text-sm font-mono">
                    {log}
                  </div>
                ))}
              </div>
            )}
          </div>

          <DialogFooter>
            <Button type="button" variant="outline" onClick={handleClose} disabled={isPending}>
              {step === 'done' ? 'Close' : 'Cancel'}
            </Button>
            {step !== 'done' && (
              <Button type="submit" disabled={isPending || !isValid}>
                {isPending ? 'Processing…' : 'Reimport and enable'}
              </Button>
            )}
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}
