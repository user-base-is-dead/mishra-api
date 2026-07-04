import { useState, useEffect } from 'react'
import { toast } from 'sonner'
import { useQueryClient } from '@tanstack/react-query'
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
  DialogDescription,
} from '@/components/ui/dialog'
import { Button } from '@/components/ui/button'
import { Switch } from '@/components/ui/switch'
import { Input } from '@/components/ui/input'
import { GroupMultiSelect } from '@/components/group-select'
import { updateCredential } from '@/api/credentials'
import type { CredentialStatusItem } from '@/types/api'

type GroupMode = 'replace' | 'add' | 'remove'

interface BatchEditCredentialDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  /** selectedaccountsobject(add/remove the mode needs to read each current groups) */
  credentials: CredentialStatusItem[]
  /** currenthasGroupoption(dedupeAggregate) */
  groupOptions: string[]
  /** callback after completion(clear the selectionetc.) */
  onDone: () => void
}

const MODE_LABELS: { value: GroupMode; label: string; desc: string }[] = [
  { value: 'replace', label: 'Replace', desc: 'Overwrite each account existing groups with the selected groups (unselect=clear groups)' },
  { value: 'add', label: 'Append', desc: 'Merge the selected groups into each account existing groups (deduplicated)' },
  { value: 'remove', label: 'Remove', desc: 'Remove the selected groups from each account groups' },
]

export function BatchEditCredentialDialog({
  open,
  onOpenChange,
  credentials,
  groupOptions,
  onDone,
}: BatchEditCredentialDialogProps) {
  const queryClient = useQueryClient()

  const [editGroups, setEditGroups] = useState(false)
  const [mode, setMode] = useState<GroupMode>('replace')
  const [groups, setGroups] = useState<string[]>([])

  const [editSource, setEditSource] = useState(false)
  const [sourceChannel, setSourceChannel] = useState('')

  const [running, setRunning] = useState(false)
  const [progress, setProgress] = useState({ current: 0, total: 0 })

  useEffect(() => {
    if (open) {
      setEditGroups(false)
      setMode('replace')
      setGroups([])
      setEditSource(false)
      setSourceChannel('')
      setRunning(false)
      setProgress({ current: 0, total: 0 })
    }
  }, [open])

  const computeGroups = (current: string[]): string[] => {
    if (mode === 'replace') return groups
    if (mode === 'add') return Array.from(new Set([...current, ...groups]))
    // remove
    return current.filter((g) => !groups.includes(g))
  }

  const handleApply = async () => {
    if (!editGroups && !editSource) {
      toast.error('Please enable at least one field to modify')
      return
    }
    setRunning(true)
    setProgress({ current: 0, total: credentials.length })
    let ok = 0
    let fail = 0
    for (let i = 0; i < credentials.length; i++) {
      const c = credentials[i]
      const req: Record<string, unknown> = {}
      if (editGroups) req.groups = computeGroups(c.groups ?? [])
      if (editSource) req.sourceChannel = sourceChannel.trim()
      try {
        await updateCredential(c.id, req)
        ok++
      } catch {
        fail++
      }
      setProgress({ current: i + 1, total: credentials.length })
    }
    await queryClient.invalidateQueries({ queryKey: ['credentials'] })
    setRunning(false)
    if (fail === 0) toast.success(`Updated ${ok} accounts`)
    else toast.warning(`Success ${ok} succeeded, failed ${fail} items`)
    onOpenChange(false)
    onDone()
  }

  return (
    <Dialog open={open} onOpenChange={(o) => !running && onOpenChange(o)}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>Bulk edit ({credentials.length} accounts)</DialogTitle>
          <DialogDescription>
            Only the enabled fields below are changed; disabled fields remain unchanged.
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-5 py-2">
          {/* Groupregion */}
          <div className="space-y-3 rounded-xl border border-border/60 p-3">
            <label className="flex items-center justify-between">
              <span className="text-sm font-medium">Edit group</span>
              <Switch checked={editGroups} onCheckedChange={setEditGroups} disabled={running} />
            </label>
            {editGroups && (
              <>
                <div className="flex gap-2">
                  {MODE_LABELS.map((m) => (
                    <Button
                      key={m.value}
                      type="button"
                      size="sm"
                      variant={mode === m.value ? 'default' : 'outline'}
                      onClick={() => setMode(m.value)}
                      disabled={running}
                    >
                      {m.label}
                    </Button>
                  ))}
                </div>
                <p className="text-[11px] text-muted-foreground">
                  {MODE_LABELS.find((m) => m.value === mode)?.desc}
                </p>
                <GroupMultiSelect
                  value={groups}
                  options={groupOptions}
                  onChange={setGroups}
                  disabled={running}
                />
              </>
            )}
          </div>

          {/* Source channelregion */}
          <div className="space-y-3 rounded-xl border border-border/60 p-3">
            <label className="flex items-center justify-between">
              <span className="text-sm font-medium">Edit source channel</span>
              <Switch checked={editSource} onCheckedChange={setEditSource} disabled={running} />
            </label>
            {editSource && (
              <>
                <Input
                  placeholder="Apply to all selected accounts (leave empty = clear)"
                  value={sourceChannel}
                  onChange={(e) => setSourceChannel(e.target.value)}
                  disabled={running}
                />
                <p className="text-[11px] text-muted-foreground">Plain note to mark the account source/channel.</p>
              </>
            )}
          </div>
        </div>

        <DialogFooter>
          <Button type="button" variant="outline" onClick={() => onOpenChange(false)} disabled={running}>
            Cancel
          </Button>
          <Button type="button" onClick={handleApply} disabled={running}>
            {running ? `Applying… ${progress.current}/${progress.total}` : 'Apply'}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
