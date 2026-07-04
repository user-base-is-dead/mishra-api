import { keepPreviousData, useQuery } from '@tanstack/react-query'
import { getByCredential, getByModel, getOverview, getTimeSeries } from '@/api/stats'
import type { StatsFilter, StatsTimeFilter } from '@/types/api'

/**
 * statistics endpointtotaluseConfig
 *
 * - `staleTime: 25_000`:30s automaticRefreshno longer triggers the backend before refetch(prevent cross Tab switch jitter)
 * - `placeholderData: keepPreviousData`:switch range or tab keep the previous duringtimesdata,
 *   chart componentInputstable reference → will not unmount and remount
 * - `refetchOnWindowFocus: false`:Admin panel lengthTimereduce instant load while pending
 */
const COMMON = {
  refetchInterval: 30_000,
  staleTime: 25_000,
  placeholderData: keepPreviousData,
  refetchOnWindowFocus: false,
} as const

export function useOverview() {
  return useQuery({
    queryKey: ['stats', 'overview'],
    queryFn: getOverview,
    ...COMMON,
  })
}

function timeKey(time: StatsTimeFilter) {
  return [
    time.range ?? 'custom',
    time.startDate ?? '',
    time.endDate ?? '',
    time.granularity,
  ] as const
}

export function useTimeSeries(time: StatsTimeFilter, filter?: StatsFilter) {
  return useQuery({
    queryKey: ['stats', 'timeseries', ...timeKey(time), filter?.keyId ?? 'all', filter?.group ?? 'all'],
    queryFn: () => getTimeSeries(time, filter),
    ...COMMON,
  })
}

export function useByModel(time: StatsTimeFilter, filter?: StatsFilter) {
  return useQuery({
    queryKey: ['stats', 'by-model', ...timeKey(time), filter?.keyId ?? 'all', filter?.group ?? 'all'],
    queryFn: () => getByModel(time, filter),
    ...COMMON,
  })
}

export function useByCredential(time: StatsTimeFilter, filter?: StatsFilter) {
  return useQuery({
    queryKey: ['stats', 'by-credential', ...timeKey(time), filter?.keyId ?? 'all', filter?.group ?? 'all'],
    queryFn: () => getByCredential(time, filter),
    ...COMMON,
  })
}
