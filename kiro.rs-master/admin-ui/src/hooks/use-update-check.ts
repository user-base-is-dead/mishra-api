import { useQuery } from '@tanstack/react-query'
import { checkSystemUpdate } from '@/api/credentials'

/**
 * poll the backend"check for updates"endpoint.
 *
 * when the backend hits cache it directlyBack,only on a miss willCallsUpstreamversion endpoint.the frontend adds another layer here
 * 15 minutesof refetchInterval,enough for the userinopenpageshortly after the pageTimesee the red dot reminder inside,
 * yet brings no obviousofRequestload.
 */
export function useUpdateCheck() {
  return useQuery({
    queryKey: ['system-update-check'],
    queryFn: () => checkSystemUpdate(false),
    // 15 minutesproactiveRefreshonetimes;firsttimesrun immediately on load
    refetchInterval: 15 * 60 * 1000,
    // avoid shortTimerepeatedly withinRequest
    staleTime: 5 * 60 * 1000,
    // do not go wild on network jitterRetry
    retry: 1,
  })
}
