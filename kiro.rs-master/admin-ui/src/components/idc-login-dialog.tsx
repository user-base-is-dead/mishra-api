import { useState, useEffect, useRef } from 'react'
import { toast } from 'sonner'
import { ExternalLink, Copy, Loader2, CheckCircle, Check } from 'lucide-react'
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogFooter,
} from '@/components/ui/dialog'
import {
  Select,
  SelectGroup,
  SelectLabel,
  SelectTrigger,
  SelectValue,
  SelectContent,
  SelectItem,
} from '@/components/ui/select'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { startIdcLogin, pollIdcLogin } from '@/api/credentials'
import type { StartIdcLoginResponse } from '@/types/api'
import { extractErrorMessage } from '@/lib/utils'

/** preset SSO Region(Group + Showname),and AWS commonRegionconsistent */
const SSO_REGION_GROUPS: { group: string; items: [string, string][] }[] = [
  {
    group: 'US',
    items: [
      ['us-east-1', 'us-east-1 (N. Virginia)'],
      ['us-east-2', 'us-east-2 (Ohio)'],
      ['us-west-1', 'us-west-1 (N. California)'],
      ['us-west-2', 'us-west-2 (Oregon)'],
    ],
  },
  {
    group: 'Europe',
    items: [
      ['eu-west-1', 'eu-west-1 (Ireland)'],
      ['eu-west-2', 'eu-west-2 (London)'],
      ['eu-west-3', 'eu-west-3 (Paris)'],
      ['eu-central-1', 'eu-central-1 (Frankfurt)'],
      ['eu-north-1', 'eu-north-1 (Stockholm)'],
      ['eu-south-1', 'eu-south-1 (Milan)'],
    ],
  },
  {
    group: 'Asia Pacific',
    items: [
      ['ap-northeast-1', 'ap-northeast-1 (Tokyo)'],
      ['ap-northeast-2', 'ap-northeast-2 (Seoul)'],
      ['ap-northeast-3', 'ap-northeast-3 (Osaka)'],
      ['ap-southeast-1', 'ap-southeast-1 (Singapore)'],
      ['ap-southeast-2', 'ap-southeast-2 (Sydney)'],
      ['ap-south-1', 'ap-south-1 (Mumbai)'],
      ['ap-east-1', 'ap-east-1 (Hong Kong)'],
    ],
  },
  {
    group: 'Other',
    items: [
      ['ca-central-1', 'ca-central-1 (Canada)'],
      ['sa-east-1', 'sa-east-1 (São Paulo)'],
      ['me-south-1', 'me-south-1 (Bahrain)'],
      ['af-south-1', 'af-south-1 (Cape Town)'],
    ],
  },
]

const KNOWN_SSO_REGIONS = SSO_REGION_GROUPS.flatMap((g) => g.items.map(([v]) => v))

/** SSO Regionselect:dropdown presetRegion + always availableInputofCustomtext box */
function RegionSelect({ value, onChange }: { value: string; onChange: (v: string) => void }) {
  const inputRef = useRef<HTMLInputElement>(null)
  const selectValue = KNOWN_SSO_REGIONS.includes(value) ? value : 'custom'
  const handleSelectChange = (v: string) => {
    if (v !== 'custom') {
      onChange(v)
      return
    }
    if (KNOWN_SSO_REGIONS.includes(value)) onChange('')
    requestAnimationFrame(() => inputRef.current?.focus())
  }

  return (
    <div className="flex gap-2">
      <Select value={selectValue} onValueChange={handleSelectChange}>
        <SelectTrigger className="flex-1 h-10">
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          {SSO_REGION_GROUPS.map((g) => (
            <SelectGroup key={g.group}>
              <SelectLabel>{g.group}</SelectLabel>
              {g.items.map(([v, label]) => (
                <SelectItem key={v} value={v}>
                  {label}
                </SelectItem>
              ))}
            </SelectGroup>
          ))}
          <SelectGroup>
            <SelectLabel>Custom</SelectLabel>
            <SelectItem value="custom">-- Custom input --</SelectItem>
          </SelectGroup>
        </SelectContent>
      </Select>
      <Input
        ref={inputRef}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        placeholder="For example: cn-north-1"
        className="w-36"
      />
    </div>
  )
}

interface IdcLoginDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  onSuccess: () => void
  /** Loginmode:'builder-id' as AWS Builder ID;'enterprise' asenterprise IAM Identity Center SSO */
  mode?: 'builder-id' | 'enterprise'
}

type Step = 'form' | 'waiting' | 'done'

export function IdcLoginDialog({ open, onOpenChange, onSuccess, mode = 'builder-id' }: IdcLoginDialogProps) {
  const isEnterprise = mode === 'enterprise'
  const [step, setStep] = useState<Step>('form')
  const [region, setRegion] = useState('us-east-1')
  const [startUrl, setStartUrl] = useState('')
  const [email, setEmail] = useState('')
  const [incognito, setIncognito] = useState(false)
  const [linkCopied, setLinkCopied] = useState(false)
  const [isStarting, setIsStarting] = useState(false)
  const [session, setSession] = useState<StartIdcLoginResponse | null>(null)
  const [credentialId, setCredentialId] = useState<number | null>(null)
  const pollTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null)

  // clear the polling timer
  useEffect(() => {
    return () => {
      if (pollTimerRef.current) clearTimeout(pollTimerRef.current)
    }
  }, [])

  // dialogClosetimeResetStatus
  const handleOpenChange = (v: boolean) => {
    if (!v) {
      if (pollTimerRef.current) clearTimeout(pollTimerRef.current)
      setStep('form')
      setSession(null)
      setCredentialId(null)
      setIsStarting(false)
      setLinkCopied(false)
    }
    onOpenChange(v)
  }

  /** Copy validation linkto the clipboard(Incognitolet the user do it manually in this modeinIncognitowindow opens) */
  const copyVerificationUrl = async (resp: StartIdcLoginResponse) => {
    const url = resp.verificationUriComplete ?? resp.verificationUri
    try {
      await navigator.clipboard.writeText(url)
      setLinkCopied(true)
      setTimeout(() => setLinkCopied(false), 2000)
      toast.success('Login link copied. Paste and open it in an incognito window')
    } catch {
      toast.error('Copy failed. Please copy the link manually')
    }
  }

  const handleStart = async () => {
    if (!region.trim()) {
      toast.error('Please fill in SSO Region')
      return
    }
    if (isEnterprise && !startUrl.trim()) {
      toast.error('Please fill in SSO Start URL')
      return
    }
    setIsStarting(true)
    try {
      const resp = await startIdcLogin({
        region: region.trim(),
        startUrl: startUrl.trim() || undefined,
        email: email.trim() || undefined,
      })
      setSession(resp)
      setStep('waiting')
      if (incognito) {
        await copyVerificationUrl(resp)
      }
      schedulePoll(resp.sessionId, resp.pollInterval)
    } catch (e) {
      toast.error('Start login failed:' + extractErrorMessage(e))
    } finally {
      setIsStarting(false)
    }
  }

  const schedulePoll = (sessionId: string, interval: number) => {
    pollTimerRef.current = setTimeout(async () => {
      try {
        const result = await pollIdcLogin(sessionId)
        if (result.status === 'pending') {
          schedulePoll(sessionId, interval)
        } else if (result.status === 'success') {
          setCredentialId(result.credentialId)
          setStep('done')
          onSuccess()
          toast.success(`Login succeeded, credential added #${result.credentialId}`)
        } else {
          toast.error('Authorization expired. Please start login again')
          setStep('form')
          setSession(null)
        }
      } catch (e) {
        toast.error('Poll status failed:' + extractErrorMessage(e))
        schedulePoll(sessionId, interval)
      }
    }, interval * 1000)
  }

  const copyCode = () => {
    if (!session) return
    navigator.clipboard.writeText(session.userCode)
    toast.success('Verification code copied')
  }

  return (
    <Dialog open={open} onOpenChange={handleOpenChange}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>
            {isEnterprise ? 'Enterprise IAM Identity Center SSO Login' : 'AWS SSO / Builder ID Login'}
          </DialogTitle>
          <DialogDescription>
            {isEnterprise
              ? 'Fill in the organization SSO Start URL and region, and add enterprise credentials through the device authorization flow.'
              : 'via AWS Identity Center Add credentials via the device authorization flow, no manual export needed refreshToken.'}
          </DialogDescription>
        </DialogHeader>

        {step === 'form' && isEnterprise && (
          <div className="space-y-4 py-2">
            <div className="space-y-1.5">
              <label htmlFor="idc-start-url" className="text-sm font-medium">
                SSO Start URL
              </label>
              <Input
                id="idc-start-url"
                placeholder="https://your-org.awsapps.com/start"
                value={startUrl}
                onChange={(e) => setStartUrl(e.target.value)}
              />
            </div>
            <div className="space-y-1.5">
              <label htmlFor="idc-region" className="text-sm font-medium">SSO Region</label>
              <RegionSelect value={region} onChange={setRegion} />
            </div>
          </div>
        )}

        {step === 'form' && !isEnterprise && (
          <div className="space-y-4 py-2">
            <div className="space-y-1.5">
              <label htmlFor="idc-region" className="text-sm font-medium">AWS Region</label>
              <RegionSelect value={region} onChange={setRegion} />
            </div>
            <div className="space-y-1.5">
              <label htmlFor="idc-start-url" className="text-sm font-medium">
                SSO Start URL
                <span className="ml-1 text-xs text-muted-foreground">
                  (leave empty to use AWS Builder ID)
                </span>
              </label>
              <Input
                id="idc-start-url"
                placeholder="https://view.awsapps.com/start"
                value={startUrl}
                onChange={(e) => setStartUrl(e.target.value)}
              />
            </div>
            <div className="space-y-1.5">
              <label htmlFor="idc-email" className="text-sm font-medium">Email (optional)</label>
              <Input
                id="idc-email"
                placeholder="user@example.com"
                value={email}
                onChange={(e) => setEmail(e.target.value)}
              />
            </div>
          </div>
        )}

        {step === 'form' && (
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
                After starting, copy the validation link and use an incognito browser yourself/Incognito window (Ctrl+Shift+N) to open,
                to avoid conflicting with the currently logged-in AWS account mix-up.
              </span>
            </span>
          </label>
        )}

        {step === 'waiting' && session && (
          <div className="space-y-4 py-2">
            <div className="rounded-lg border bg-muted/50 p-4 text-center space-y-3">
              {incognito ? (
                <>
                  <p className="text-sm text-muted-foreground">
                    Validation link copied. Please open a new
                    <span className="font-medium text-foreground">Incognito / Incognito window</span>
                    (Ctrl+Shift+N,Safari as ⌘+Shift+N), paste to open and complete authorization.
                  </p>
                  <Button
                    size="sm"
                    variant="outline"
                    onClick={() => copyVerificationUrl(session)}
                  >
                    {linkCopied ? (
                      <Check className="h-3.5 w-3.5" />
                    ) : (
                      <Copy className="h-3.5 w-3.5" />
                    )}
                    {linkCopied ? 'Copied' : 'Copy validation link'}
                  </Button>
                </>
              ) : (
                <>
                  <p className="text-sm text-muted-foreground">Open the following address in your browser and enter the verification code</p>
                  <a
                    href={session.verificationUriComplete ?? session.verificationUri}
                    target="_blank"
                    rel="noopener noreferrer"
                    className="inline-flex items-center gap-1.5 text-sm font-medium text-primary hover:underline"
                  >
                    {session.verificationUri}
                    <ExternalLink className="h-3.5 w-3.5" />
                  </a>
                </>
              )}
              <div className="flex items-center justify-center gap-2">
                <span className="font-mono text-2xl font-bold tracking-widest">
                  {session.userCode}
                </span>
                <Button variant="ghost" size="icon" className="h-7 w-7" onClick={copyCode}>
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

        {step === 'done' && (
          <div className="flex flex-col items-center gap-3 py-4">
            <CheckCircle className="h-10 w-10 text-green-500" />
            <p className="text-sm font-medium">Login succeeded</p>
            <p className="text-xs text-muted-foreground">
              Credential #{credentialId} Added and enabled
            </p>
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
            <Button variant="outline" onClick={() => handleOpenChange(false)}>
              Cancel
            </Button>
          )}
          {step === 'done' && (
            <Button onClick={() => handleOpenChange(false)}>Close</Button>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
