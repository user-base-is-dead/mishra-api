import { useState } from 'react'
import { toast } from 'sonner'
import {
  Plus, FolderTree, Trash2, Pencil, Users, KeyRound, RefreshCw,
} from 'lucide-react'
import { Card, CardContent } from '@/components/ui/card'
import { Button } from '@/components/ui/button'
import { Badge } from '@/components/ui/badge'
import { Input } from '@/components/ui/input'
import {
  Dialog, DialogContent, DialogHeader, DialogTitle, DialogFooter, DialogDescription,
} from '@/components/ui/dialog'
import {
  useGroups, useCreateGroup, useUpdateGroup, useDeleteGroup,
} from '@/hooks/use-groups'
import { useConfirm } from '@/components/ui/confirm-dialog'
import { extractErrorMessage } from '@/lib/utils'
import type { GroupItem } from '@/types/api'

/**
 * Group managementpage:CRUD registeredGroup.
 *
 * design point:
 * - Groupis an independent entity,Credential / Client key Key vianame reference
 * - Renamego through cascade(the backend syncs automaticallyhasreference)
 * - DeleteDefaultrejecthasreferenceof,twotimesConfirmonly then allow force cascade cleanup
 * - Listshow eachgroupscurrentlyhow manycredentials / Key reference,Deleteclearly know the impact beforehand
 */
export function GroupsPage() {
  const { data, isLoading, isFetching, refetch } = useGroups()
  const createGroup = useCreateGroup()
  const updateGroup = useUpdateGroup()
  const deleteGroup = useDeleteGroup()
  const confirm = useConfirm()

  const [createOpen, setCreateOpen] = useState(false)
  const [createName, setCreateName] = useState('')
  const [createDesc, setCreateDesc] = useState('')

  const [editOpen, setEditOpen] = useState(false)
  const [editTarget, setEditTarget] = useState<GroupItem | null>(null)
  const [editNewName, setEditNewName] = useState('')
  const [editDesc, setEditDesc] = useState('')

  const groups = data?.groups ?? []

  const openCreate = () => {
    setCreateName('')
    setCreateDesc('')
    setCreateOpen(true)
  }

  const handleCreate = async () => {
    const name = createName.trim()
    if (!name) {
      toast.error('Group name cannot be empty')
      return
    }
    try {
      await createGroup.mutateAsync({
        name,
        description: createDesc.trim() || undefined,
      })
      toast.success(`Group created:${name}`)
      setCreateOpen(false)
    } catch (e) {
      toast.error(extractErrorMessage(e))
    }
  }

  const openEdit = (g: GroupItem) => {
    setEditTarget(g)
    setEditNewName(g.name)
    setEditDesc(g.description ?? '')
    setEditOpen(true)
  }

  const handleEdit = async () => {
    if (!editTarget) return
    const newName = editNewName.trim()
    if (!newName) {
      toast.error('Group name cannot be empty')
      return
    }
    try {
      await updateGroup.mutateAsync({
        name: editTarget.name,
        req: {
          newName: newName !== editTarget.name ? newName : undefined,
          description: editDesc, // emptystring → the backend clears
        },
      })
      const renamed = newName !== editTarget.name
      toast.success(renamed ? `Renamed:${editTarget.name} → ${newName}` : 'Note updated')
      setEditOpen(false)
    } catch (e) {
      toast.error(extractErrorMessage(e))
    }
  }

  const handleDelete = async (g: GroupItem) => {
    const refs = g.credentialCount + g.clientKeyCount
    // no reference:single layerConfirm;hasreference:twotimesConfirm + force
    if (refs === 0) {
      const ok = await confirm({
        title: `Delete group ${g.name}?`,
        description: 'This group has no references and can be safely deleted.',
        confirmText: 'Delete',
        destructive: true,
      })
      if (!ok) return
      try {
        await deleteGroup.mutateAsync({ name: g.name })
        toast.success(`Group ${g.name} Deleted`)
      } catch (e) {
        toast.error(extractErrorMessage(e))
      }
    } else {
      const ok = await confirm({
        title: `Force delete group ${g.name}?`,
        description: `This group is currently ${g.credentialCount} credentials + ${g.clientKeyCount} put the client key Key references. Continuing cascades to clean up these references (credentials from groups removes this group from the list; the client key Key unbind). This action cannot be undone.`,
        confirmText: 'Force delete',
        destructive: true,
      })
      if (!ok) return
      try {
        await deleteGroup.mutateAsync({ name: g.name, force: true })
        toast.success(`Group ${g.name} Deleted, cleaned up ${refs} references`)
      } catch (e) {
        toast.error(extractErrorMessage(e))
      }
    }
  }

  return (
    <div className="space-y-4">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <div>
          <h2 className="text-lg font-semibold flex items-center gap-2">
            <FolderTree className="h-4 w-4" />
            Group management
          </h2>
          <p className="text-sm text-muted-foreground mt-1">
            Groups are credentials / Client key Key a shared independent entity; renaming / Deletion cascades in sync.
          </p>
        </div>
        <div className="flex items-center gap-2">
          <Button size="sm" variant="outline" onClick={() => refetch()} disabled={isFetching}>
            <RefreshCw className={`h-3.5 w-3.5 ${isFetching ? 'animate-spin' : ''}`} />
            Refresh
          </Button>
          <Button size="sm" onClick={openCreate}>
            <Plus className="h-3.5 w-3.5" />
            New group
          </Button>
        </div>
      </div>

      {isLoading ? (
        <Card><CardContent className="py-8 text-sm text-center text-muted-foreground">Loading…</CardContent></Card>
      ) : groups.length === 0 ? (
        <Card>
          <CardContent className="py-12 text-sm text-center text-muted-foreground space-y-2">
            <FolderTree className="h-8 w-8 mx-auto opacity-40" />
            <p>No groups yet. Click New group above to start.</p>
          </CardContent>
        </Card>
      ) : (
        <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
          {groups.map((g) => (
            <Card key={g.name}>
              <CardContent className="p-4 space-y-3">
                <div className="flex items-start justify-between gap-2">
                  <div className="min-w-0">
                    <div className="font-medium truncate">{g.name}</div>
                    {g.description && (
                      <p className="text-xs text-muted-foreground mt-0.5 line-clamp-2">{g.description}</p>
                    )}
                  </div>
                  <div className="flex shrink-0 items-center gap-1">
                    <Button size="icon" variant="ghost" className="h-7 w-7" onClick={() => openEdit(g)} title="Edit">
                      <Pencil className="h-3.5 w-3.5" />
                    </Button>
                    <Button
                      size="icon"
                      variant="ghost"
                      className="h-7 w-7 text-destructive hover:text-destructive"
                      onClick={() => handleDelete(g)}
                      title="Delete"
                    >
                      <Trash2 className="h-3.5 w-3.5" />
                    </Button>
                  </div>
                </div>

                <div className="flex flex-wrap items-center gap-2 text-xs">
                  <Badge variant="secondary" className="gap-1">
                    <Users className="h-3 w-3" />
                    {g.credentialCount} Credential
                  </Badge>
                  <Badge variant="secondary" className="gap-1">
                    <KeyRound className="h-3 w-3" />
                    {g.clientKeyCount} Key
                  </Badge>
                </div>

                <p className="text-[11px] text-muted-foreground">
                  Created at {new Date(g.createdAt).toLocaleString()}
                </p>
              </CardContent>
            </Card>
          ))}
        </div>
      )}

      {/* New groupdialog */}
      <Dialog open={createOpen} onOpenChange={setCreateOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>New group</DialogTitle>
            <DialogDescription>
              Once registered, you can in the credential / Client key Key select this group in.
            </DialogDescription>
          </DialogHeader>
          <div className="space-y-3">
            <div className="space-y-1">
              <label className="text-sm font-medium">Group name *</label>
              <Input
                placeholder="For example: customerA, production, backup pool"
                value={createName}
                onChange={(e) => setCreateName(e.target.value)}
                disabled={createGroup.isPending}
                autoFocus
              />
            </div>
            <div className="space-y-1">
              <label className="text-sm font-medium">Note (optional)</label>
              <Input
                placeholder="Purpose description for easier identification later"
                value={createDesc}
                onChange={(e) => setCreateDesc(e.target.value)}
                disabled={createGroup.isPending}
              />
            </div>
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setCreateOpen(false)} disabled={createGroup.isPending}>
              Cancel
            </Button>
            <Button onClick={handleCreate} disabled={createGroup.isPending || !createName.trim()}>
              {createGroup.isPending ? 'Creating…' : 'Create'}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* EditGroupdialog */}
      <Dialog open={editOpen} onOpenChange={setEditOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Edit group:{editTarget?.name}</DialogTitle>
            <DialogDescription>
              Renaming cascades to all credentials and client keys that reference this group Key.
            </DialogDescription>
          </DialogHeader>
          <div className="space-y-3">
            <div className="space-y-1">
              <label className="text-sm font-medium">Group name</label>
              <Input
                value={editNewName}
                onChange={(e) => setEditNewName(e.target.value)}
                disabled={updateGroup.isPending}
              />
            </div>
            <div className="space-y-1">
              <label className="text-sm font-medium">Note</label>
              <Input
                placeholder="(leave empty to clear the note)"
                value={editDesc}
                onChange={(e) => setEditDesc(e.target.value)}
                disabled={updateGroup.isPending}
              />
            </div>
            {editTarget && (editTarget.credentialCount > 0 || editTarget.clientKeyCount > 0) && (
              <p className="text-xs text-amber-600">
                currently {editTarget.credentialCount} Credential + {editTarget.clientKeyCount} Client key Key references; renaming syncs automatically.
              </p>
            )}
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setEditOpen(false)} disabled={updateGroup.isPending}>
              Cancel
            </Button>
            <Button onClick={handleEdit} disabled={updateGroup.isPending || !editNewName.trim()}>
              {updateGroup.isPending ? 'Saving…' : 'Save'}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  )
}
