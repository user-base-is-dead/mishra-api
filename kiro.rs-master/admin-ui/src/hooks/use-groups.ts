import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import {
  listGroups,
  createGroup,
  deleteGroup,
  updateGroup,
} from '@/api/groups'
import type { CreateGroupRequest, UpdateGroupRequest } from '@/types/api'

export function useGroups() {
  return useQuery({
    queryKey: ['groups'],
    queryFn: listGroups,
    // Grouplow change frequency(manualActions),15s automaticRefreshenough
    refetchInterval: 15000,
    staleTime: 5000,
  })
}

/**
 * give allhas GroupSelect useof"registeredGroup name"stringarray.
 * internal reuse useGroups cache,will notDuplicatecall the endpoint.
 */
export function useGroupOptions(): string[] {
  const { data } = useGroups()
  return (data?.groups ?? []).map((g) => g.name)
}

export function useCreateGroup() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (req: CreateGroupRequest) => createGroup(req),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['groups'] }),
  })
}

export function useUpdateGroup() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: ({ name, req }: { name: string; req: UpdateGroupRequest }) =>
      updateGroup(name, req),
    onSuccess: () => {
      // Rename / changeNotewill affectCredential / Key ofdisplay,three cachesAllInvalid
      qc.invalidateQueries({ queryKey: ['groups'] })
      qc.invalidateQueries({ queryKey: ['credentials'] })
      qc.invalidateQueries({ queryKey: ['client-keys'] })
    },
  })
}

export function useDeleteGroup() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: ({ name, force }: { name: string; force?: boolean }) =>
      deleteGroup(name, !!force),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['groups'] })
      qc.invalidateQueries({ queryKey: ['credentials'] })
      qc.invalidateQueries({ queryKey: ['client-keys'] })
    },
  })
}
