import { keepPreviousData, useQuery } from '@tanstack/react-query'
import { getTraces, getFailureStats } from '@/api/traces'
import type { TraceQuery } from '@/types/api'

/**
 * Requesttrace query hook
 *
 * reuse and stats consistentofRefreshpolicy:30s automaticRefresh,keep old data when switching filters to avoid flicker.
 * `enabled=false` do not send at that timeRequest(Used forwhen the dialog is not openoflazy load).
 */
export function useTraces(query: TraceQuery, enabled = true) {
  return useQuery({
    queryKey: ['traces', query],
    queryFn: () => getTraces(query),
    enabled,
    refetchInterval: enabled ? 30_000 : false,
    staleTime: 10_000,
    placeholderData: keepPreviousData,
    refetchOnWindowFocus: false,
  })
}

/** byCredentialofFailedcount by category(authenticate/throttle/other),Used forCardcolor coded display */
export function useFailureStats() {
  return useQuery({
    queryKey: ['traces', 'failure-stats'],
    queryFn: getFailureStats,
    refetchInterval: 30_000,
    staleTime: 10_000,
    refetchOnWindowFocus: false,
  })
}
