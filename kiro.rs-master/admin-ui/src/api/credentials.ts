import axios from 'axios'
import { storage } from '@/lib/storage'
import type {
  CredentialsStatusResponse,
  BalanceResponse,
  AvailableModelsResponse,
  SuccessResponse,
  SetDisabledRequest,
  SetPriorityRequest,
  AddCredentialRequest,
  AddCredentialResponse,
  UpdateCredentialRequest,
  UpdateRefreshTokenRequest,
  ProxyPoolEntry,
  ProxyPoolResponse,
  AddProxyRequest,
  BatchAddProxyRequest,
  BatchAddProxyResponse,
  AssignProxyRequest,
  ProxyCheckResponse,
  ProxyCheckAllResponse,
  AssignRoundRobinResponse,
  StartIdcLoginRequest,
  StartIdcLoginResponse,
  PollIdcLoginResponse,
  StartSocialLoginRequest,
  StartSocialLoginResponse,
  PollSocialLoginResponse,
  CompleteSocialLoginRequest,
  GlobalProxyResponse,
  SetGlobalProxyRequest,
  UpdateConfigResponse,
  SetUpdateConfigRequest,
  ImageUpdateResponse,
  UpdateCheckInfo,
  GitHubRateLimitInfo,
  UpdateAdminKeyRequest,
} from '@/types/api'

// Create axios actuale.g.
const api = axios.create({
  baseURL: '/api/admin',
  timeout: 15000,
  headers: {
    'Content-Type': 'application/json',
  },
})

/**
 * based on the current browser access addressAuto-derive OAuth callback public address.
 *
 * panel and API same origin(API use relative /api/admin prefix),so the browser itself knowsof origin is the most trustworthyofpublic address.
 * browserAuthorizewill land at after `${origin}/api/admin/auth/callback/oauth/callback`,byserver public callback pathbyreceive.
 * remote deployment(Render / VPS / Docker)zeroConfignamelyAvailable;if a forced override is needed,inbackend config.json match callbackBaseUrl.
 */
function deriveCallbackBaseUrl(): string {
  return `${window.location.origin}/api/admin/auth/callback`
}

// RequestinterceptorAdd API Key
api.interceptors.request.use((config) => {
  const apiKey = storage.getApiKey()
  if (apiKey) {
    config.headers['x-api-key'] = apiKey
  }
  return config
})

// get allhasCredentialStatus
export async function getCredentials(): Promise<CredentialsStatusResponse> {
  const { data } = await api.get<CredentialsStatusResponse>('/credentials')
  return data
}

// ============ KAM Export ============

/** KAM ExportAccount(KAM 1.8.3+ flat layoutFormat) */
export interface KamExportAccount {
  email?: string
  nickname?: string
  idp?: string
  provider?: string
  status?: string
  authMethod?: string
  region?: string
  startUrl?: string
  clientId?: string
  clientSecret?: string
  refreshToken?: string
  accessToken?: string
  profileArn?: string
  expiresAt?: string
  machineId?: string
}

export interface KamExportResponse {
  version: string
  exportedAt: string
  accounts: KamExportAccount[]
}

/** ExportCredentialas KAM compatible JSON(including refreshToken etc.sensitive field).
 *  pass in `ids` at that time onlyExporttheseCredential;if omitted thenExportAll. */
export async function exportKamCredentials(
  ids?: number[]
): Promise<KamExportResponse> {
  const params = ids && ids.length > 0 ? { ids: ids.join(',') } : undefined
  const { data } = await api.get<KamExportResponse>('/credentials/export', { params })
  return data
}

// SettingsCredentialDisableStatus
export async function setCredentialDisabled(
  id: number,
  disabled: boolean
): Promise<SuccessResponse> {
  const { data } = await api.post<SuccessResponse>(
    `/credentials/${id}/disabled`,
    { disabled } as SetDisabledRequest
  )
  return data
}

// SettingsCredentialPriority
export async function setCredentialPriority(
  id: number,
  priority: number
): Promise<SuccessResponse> {
  const { data } = await api.post<SuccessResponse>(
    `/credentials/${id}/priority`,
    { priority } as SetPriorityRequest
  )
  return data
}

// Reset failure count
export async function resetCredentialFailure(
  id: number
): Promise<SuccessResponse> {
  const { data } = await api.post<SuccessResponse>(`/credentials/${id}/reset`)
  return data
}

// Force refresh Token
export async function forceRefreshToken(
  id: number
): Promise<SuccessResponse> {
  const { data } = await api.post<SuccessResponse>(`/credentials/${id}/refresh`)
  return data
}

// releaseCredentialaccountslevel throttleCooldown
export async function clearThrottle(id: number): Promise<SuccessResponse> {
  const { data } = await api.post<SuccessResponse>(`/credentials/${id}/clear-throttle`)
  return data
}

// getCredentialBalance
export async function getCredentialBalance(id: number): Promise<BalanceResponse> {
  const { data } = await api.get<BalanceResponse>(`/credentials/${id}/balance`)
  return data
}

// getCredentialcurrentAvailableofModelList(query in real time on demandUpstream)
export async function getCredentialModels(id: number): Promise<AvailableModelsResponse> {
  const { data } = await api.get<AvailableModelsResponse>(`/credentials/${id}/models`)
  return data
}

// AddnewCredential
export async function addCredential(
  req: AddCredentialRequest
): Promise<AddCredentialResponse> {
  const { data } = await api.post<AddCredentialResponse>('/credentials', req)
  return data
}

// ── Bulk import(SSE) ──────────────────────────────────────────────────────────

/** Bulk import SSE singleitemsevent(correspondingRequestarray index index) */
export interface BatchImportItemEvent {
  index: number
  status: 'verified' | 'imported' | 'duplicate' | 'failed'
  credentialId?: number
  email?: string
  usage?: string
  subscription?: string
  error?: string
  /** failed and already rolled back(Delete)timeas true */
  rolledBack?: boolean
}

/** Bulk importsummary at the endtotalevent */
export interface BatchImportSummary {
  total: number
  /** Import directly(not validated)Successcount */
  imported: number
  verified: number
  duplicate: number
  failed: number
  rolledBack: number
}

export interface BatchImportCredentialsRequest {
  credentials: AddCredentialRequest[]
  /** concurrency,default 8,server clamp to [1, 16] */
  concurrency?: number
  /** whether to validate.true(default):add take afterBalancevalidate + Failedrollback;false:only add persist to database(Import directly) */
  verify?: boolean
}

/**
 * Bulk import credentialsand validate(SSE stream).
 *
 * serverhasconcurrently one by oneitems add + takeBalancevalidate + Failedrollback,eachitemsonce completevia SSE push
 * oneitems `BatchImportItemEvent`(out of order,carry index),Allpush one after completionitemssummarytotal.
 *
 * use fetch read the stream whilenot EventSource:EventSource notSupported POST/Custom header,
 * while thisEndpointmust carry x-api-key authenticate and POST large body.
 *
 * @param onEvent      eachitemsCredentialresult
 * @param onSummary    summary at the endtotal
 * @param signal       AbortSignal,CanceltimeInterruptedstream read
 */
export async function batchImportCredentials(
  req: BatchImportCredentialsRequest,
  onEvent: (e: BatchImportItemEvent) => void,
  onSummary: (s: BatchImportSummary) => void,
  signal?: AbortSignal,
): Promise<void> {
  const apiKey = storage.getApiKey()
  const resp = await fetch('/api/admin/credentials/batch-import', {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      ...(apiKey ? { 'x-api-key': apiKey } : {}),
    },
    body: JSON.stringify(req),
    signal,
  })

  if (!resp.ok) {
    let msg = `HTTP ${resp.status}`
    try {
      const body = await resp.json()
      msg = body?.message || body?.error || msg
    } catch {
      /* ignore JSON Parse failed,Roll back toStatuscode */
    }
    throw new Error(msg)
  }
  if (!resp.body) throw new Error('The response lacks a readable stream')

  const reader = resp.body.getReader()
  const decoder = new TextDecoder()
  let buffer = ''

  for (;;) {
    const { done, value } = await reader.read()
    if (done) break
    buffer += decoder.decode(value, { stream: true })

    // SSE events with a blank line(\n\n)separator
    let sep: number
    while ((sep = buffer.indexOf('\n\n')) !== -1) {
      const raw = buffer.slice(0, sep)
      buffer = buffer.slice(sep + 2)
      const dataLine = raw.split('\n').find((l) => l.startsWith('data:'))
      if (!dataLine) continue
      const jsonStr = dataLine.slice(5).trim()
      if (!jsonStr) continue
      let ev: Record<string, unknown>
      try {
        ev = JSON.parse(jsonStr)
      } catch {
        continue
      }
      if (ev.status === 'summary') {
        onSummary(ev.summary as BatchImportSummary)
      } else {
        onEvent(ev as unknown as BatchImportItemEvent)
      }
    }
  }
}

// Delete credential
export async function deleteCredential(id: number): Promise<SuccessResponse> {
  const { data } = await api.delete<SuccessResponse>(`/credentials/${id}`)
  return data
}

// ResetsinglecredentialsSuccess count
export async function resetSuccessCount(id: number): Promise<SuccessResponse> {
  const { data } = await api.post<SuccessResponse>(`/credentials/${id}/reset-stats`)
  return data
}

// ResetplacehasCredentialofSuccess count
export async function resetAllSuccessCount(): Promise<SuccessResponse> {
  const { data } = await api.post<SuccessResponse>('/credentials/reset-stats')
  return data
}

// one clickDisableplacehas"Over quota"Credential
export interface QuotaExceededResult {
  disabledIds: number[]
  skippedIds: number[]
}
export async function disableQuotaExceeded(): Promise<QuotaExceededResult> {
  const { data } = await api.post<QuotaExceededResult>('/credentials/disable-quota-exceeded')
  return data
}

// SettingssinglecredentialsOveragetoggle
export async function setCredentialOverage(id: number, enabled: boolean): Promise<SuccessResponse> {
  const { data } = await api.post<SuccessResponse>(`/credentials/${id}/overage`, { enabled })
  return data
}

// one clickEnableplacehascanEnable overageofCredential
export interface EnableOverageAllResult {
  enabledIds: number[]
  skippedIds: number[]
  failedIds: number[]
  failureMessages: string[]
}
export async function enableOverageForAllCapable(): Promise<EnableOverageAllResult> {
  const { data } = await api.post<EnableOverageAllResult>('/credentials/overage/enable-all')
  return data
}

// updateDisabledCredentialof refreshToken
export async function updateRefreshToken(
  id: number,
  req: UpdateRefreshTokenRequest
): Promise<SuccessResponse> {
  const { data } = await api.put<SuccessResponse>(`/credentials/${id}/refresh-token`, req)
  return data
}

// updateCredentialcanEditfield
export async function updateCredential(
  id: number,
  req: UpdateCredentialRequest
): Promise<SuccessResponse> {
  const { data } = await api.put<SuccessResponse>(`/credentials/${id}`, req)
  return data
}

// ============ Proxy pool ============

// getProxy poolList
export async function getProxyPool(): Promise<ProxyPoolResponse> {
  const { data } = await api.get<ProxyPoolResponse>('/proxy-pool')
  return data
}

// AddProxy
export async function addProxy(req: AddProxyRequest): Promise<ProxyPoolEntry> {
  const { data } = await api.post<ProxyPoolEntry>('/proxy-pool', req)
  return data
}

// BulkAddProxy
export async function batchAddProxies(req: BatchAddProxyRequest): Promise<BatchAddProxyResponse> {
  const { data } = await api.post<BatchAddProxyResponse>('/proxy-pool/batch', req)
  return data
}

// DeleteProxy
export async function deleteProxy(id: number): Promise<SuccessResponse> {
  const { data } = await api.delete<SuccessResponse>(`/proxy-pool/${id}`)
  return data
}

// SettingsProxyEnable/Disable
export async function setProxyEnabled(id: number, enabled: boolean): Promise<SuccessResponse> {
  const { data } = await api.post<SuccessResponse>(`/proxy-pool/${id}/enabled`, { enabled })
  return data
}

// assignProxygiveCredential
export async function assignProxyToCredential(
  credentialId: number,
  req: AssignProxyRequest
): Promise<SuccessResponse> {
  const { data } = await api.post<SuccessResponse>(`/credentials/${credentialId}/proxy`, req)
  return data
}

// probe instantly a singleproxiesconnectivity
export async function checkProxy(id: number): Promise<ProxyCheckResponse> {
  const { data } = await api.post<ProxyCheckResponse>(`/proxy-pool/${id}/check`)
  return data
}

// triggerAllProxyhealth check
export async function checkAllProxies(): Promise<ProxyCheckAllResponse> {
  const { data } = await api.post<ProxyCheckAllResponse>('/proxy-pool/check-all')
  return data
}

// pollBulkassignAvailableProxygiveCredential
export async function assignProxiesRoundRobin(
  credentialIds?: number[] | null
): Promise<AssignRoundRobinResponse> {
  const { data } = await api.post<AssignRoundRobinResponse>('/proxy-pool/assign-round-robin', {
    credentialIds: credentialIds ?? null,
  })
  return data
}

// get the load balancing mode
export async function getLoadBalancingMode(): Promise<{ mode: 'priority' | 'balanced' }> {
  const { data } = await api.get<{ mode: 'priority' | 'balanced' }>('/config/load-balancing')
  return data
}

// Settingsload balancing mode
export async function setLoadBalancingMode(mode: 'priority' | 'balanced'): Promise<{ mode: 'priority' | 'balanced' }> {
  const { data } = await api.put<{ mode: 'priority' | 'balanced' }>('/config/load-balancing', { mode })
  return data
}

export interface AccountThrottleConfig {
  failover: boolean
  cooldownSecs: number
}

// getAccount-level throttle failoverConfig
export async function getAccountThrottleConfig(): Promise<AccountThrottleConfig> {
  const { data } = await api.get<AccountThrottleConfig>('/config/account-throttle')
  return data
}

// updateAccount-level throttle failoverConfig
export async function setAccountThrottleConfig(
  patch: Partial<AccountThrottleConfig>,
): Promise<AccountThrottleConfig> {
  const { data } = await api.put<AccountThrottleConfig>('/config/account-throttle', patch)
  return data
}

export interface LogGovernanceConfig {
  traceEnabled: boolean
  traceRetentionDays: number
  usageLogRetentionDays: number
}

// getLogsgovernanceConfig
export async function getLogGovernanceConfig(): Promise<LogGovernanceConfig> {
  const { data } = await api.get<LogGovernanceConfig>('/config/log-governance')
  return data
}

// updateLogsgovernanceConfig
export async function setLogGovernanceConfig(
  patch: Partial<LogGovernanceConfig>,
): Promise<LogGovernanceConfig> {
  const { data } = await api.put<LogGovernanceConfig>('/config/log-governance', patch)
  return data
}

// initiate IdC Device authorizationLogin
export async function startIdcLogin(
  req: StartIdcLoginRequest
): Promise<StartIdcLoginResponse> {
  const { data } = await api.post<StartIdcLoginResponse>('/auth/idc/start', req)
  return data
}

// poll IdC LoginStatus
export async function pollIdcLogin(sessionId: string): Promise<PollIdcLoginResponse> {
  const { data } = await api.post<PollIdcLoginResponse>(`/auth/idc/poll/${sessionId}`)
  return data
}

// getGlobalProxy config
export async function getGlobalProxy(): Promise<GlobalProxyResponse> {
  const { data } = await api.get<GlobalProxyResponse>('/config/global-proxy')
  return data
}

// SettingsGlobalProxy config
export async function setGlobalProxy(req: SetGlobalProxyRequest): Promise<SuccessResponse> {
  const { data } = await api.put<SuccessResponse>('/config/global-proxy', req)
  return data
}

// getMirror online updateConfig
export async function getUpdateConfig(): Promise<UpdateConfigResponse> {
  const { data } = await api.get<UpdateConfigResponse>('/config/update')
  return data
}

// SettingsMirror online updateConfig
export async function setUpdateConfig(req: SetUpdateConfigRequest): Promise<UpdateConfigResponse> {
  const { data } = await api.put<UpdateConfigResponse>('/config/update', req)
  return data
}

// fetchConfigof GHCR mirror
export async function pullUpdateImage(): Promise<ImageUpdateResponse> {
  const { data } = await api.post<ImageUpdateResponse>('/system/update/pull')
  return data
}

// Pull imageandvia Docker Compose Applyupdate
export async function applyImageUpdate(): Promise<ImageUpdateResponse> {
  const { data } = await api.post<ImageUpdateResponse>('/system/update/apply')
  return data
}

// vialocal backup tag Roll back toprevioustimesbefore updateofmirror version
export async function rollbackImageUpdate(): Promise<ImageUpdateResponse> {
  const { data } = await api.post<ImageUpdateResponse>('/system/update/rollback')
  return data
}

// check GitHub Releases whetherhasnew version(with backend 30 minutescache;force=true Force refresh)
export async function checkSystemUpdate(force = false): Promise<UpdateCheckInfo> {
  const { data } = await api.get<UpdateCheckInfo>('/system/update/check', {
    params: force ? { force: 'true' } : undefined,
  })
  return data
}

// query GitHub API currentRate limitStatus(can attach token Used for"Savefirst beforeValidate")
export async function checkGitHubRateLimit(
  githubToken?: string,
): Promise<GitHubRateLimitInfo> {
  const body = githubToken ? { githubToken } : {}
  const { data } = await api.post<GitHubRateLimitInfo>(
    '/system/update/rate-limit',
    body,
  )
  return data
}

// Change loginAPIKey(adminApiKey —— ManagepanelLoginKey)
export async function updateAdminKey(req: UpdateAdminKeyRequest): Promise<SuccessResponse> {
  const { data } = await api.put<SuccessResponse>('/config/admin-key', req)
  return data
}

// initiate Social Login
export async function startSocialLogin(
  req: StartSocialLoginRequest
): Promise<StartSocialLoginResponse> {
  const { data } = await api.post<StartSocialLoginResponse>('/auth/social/start', {
    callbackBaseUrl: deriveCallbackBaseUrl(),
    ...req,
  })
  return data
}

// poll Social LoginStatus
export async function pollSocialLogin(sessionId: string): Promise<PollSocialLoginResponse> {
  const { data } = await api.post<PollSocialLoginResponse>(`/auth/social/poll/${sessionId}`)
  return data
}

// complete manually Social Login (on remote accessPastecallback URL)
export async function completeSocialLogin(
  sessionId: string,
  req: CompleteSocialLoginRequest
): Promise<PollSocialLoginResponse> {
  const { data } = await api.post<PollSocialLoginResponse>(`/auth/social/complete/${sessionId}`, req)
  return data
}

// ============ Log in again(update alreadyhasCredential Token) ============

// initiate Social Log in again
export async function startSocialRelogin(
  credentialId: number,
  req: StartSocialLoginRequest
): Promise<StartSocialLoginResponse> {
  const { data } = await api.post<StartSocialLoginResponse>(
    `/credentials/${credentialId}/relogin/social/start`,
    {
      callbackBaseUrl: deriveCallbackBaseUrl(),
      ...req,
    }
  )
  return data
}

// poll Social Log in againStatus
export async function pollSocialRelogin(
  credentialId: number,
  sessionId: string
): Promise<PollSocialLoginResponse> {
  const { data } = await api.post<PollSocialLoginResponse>(
    `/credentials/${credentialId}/relogin/social/poll/${sessionId}`
  )
  return data
}

// complete manually Social Log in again(on remote accessPastecallback URL)
export async function completeSocialRelogin(
  credentialId: number,
  sessionId: string,
  req: CompleteSocialLoginRequest
): Promise<PollSocialLoginResponse> {
  const { data } = await api.post<PollSocialLoginResponse>(
    `/credentials/${credentialId}/relogin/social/complete/${sessionId}`,
    req
  )
  return data
}

// initiate IdC Log in again
export async function startIdcRelogin(
  credentialId: number,
  req: StartIdcLoginRequest
): Promise<StartIdcLoginResponse> {
  const { data } = await api.post<StartIdcLoginResponse>(
    `/credentials/${credentialId}/relogin/idc/start`,
    req
  )
  return data
}

// poll IdC Log in againStatus
export async function pollIdcRelogin(
  credentialId: number,
  sessionId: string
): Promise<PollIdcLoginResponse> {
  const { data } = await api.post<PollIdcLoginResponse>(
    `/credentials/${credentialId}/relogin/idc/poll/${sessionId}`
  )
  return data
}
