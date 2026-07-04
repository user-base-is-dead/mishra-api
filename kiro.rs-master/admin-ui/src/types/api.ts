// CredentialStatusresponse
export interface CredentialsStatusResponse {
  total: number
  available: number
  currentId: number
  credentials: CredentialStatusItem[]
}

// singlecredentialsStatus
export interface CredentialStatusItem {
  id: number
  priority: number
  disabled: boolean
  failureCount: number
  /** accumulateFailure count(placehasFailedtype,only increases never decreases,manual onlyResetreset to zero) */
  totalFailureCount: number
  isCurrent: boolean
  expiresAt: string | null
  authMethod: string | null
  provider?: string | null
  hasProfileArn: boolean
  email?: string
  refreshTokenHash?: string
  apiKeyHash?: string
  maskedApiKey?: string
  successCount: number
  lastUsedAt: string | null
  hasProxy: boolean
  proxyUrl?: string
  refreshFailureCount: number
  disabledReason?: string
  /** Accountlevel throttleCooldownRemainingseconds(>0 representsCooldownin) */
  throttledRemainingSecs?: number
  endpoint: string
  /** Accountbelongs toGroup(can belong to manygroups) */
  groups?: string[]
  /** Account source channel(plainNote) */
  sourceChannel?: string
  /** backend cacheofmostrecentonetimesBalance(5 minutesinside) */
  balance?: BalanceResponse
  /** BalancecacheofupdateTime(Unix second) */
  balanceUpdatedAt?: number
}

// Balanceresponse
export interface BalanceResponse {
  id: number
  subscriptionTitle: string | null
  currentUsage: number
  usageLimit: number
  remaining: number
  usagePercentage: number
  nextResetAt: number | null
  /** whether the user is currentlyEnabledoneOverage */
  overageEnabled?: boolean
  /** Accountwhether the subscription canEnable overage */
  overageCapable?: boolean
  /** Upstream overageCapability rawstring,Used fortroubleshoot"Unknown"Status */
  overageCapabilityRaw?: string
}

// someCredentialcurrentAvailableofModelListresponse
export interface AvailableModelsResponse {
  id: number
  models: AvailableModelItem[]
}

// singleitemsAvailable models
export interface AvailableModelItem {
  modelId: string
  modelName?: string
  description?: string
  maxInputTokens?: number
}

// Successresponse
export interface SuccessResponse {
  success: boolean
  message: string
}

// Errorresponse
export interface AdminErrorResponse {
  error: {
    type: string
    message: string
  }
}

// Requesttype
export interface SetDisabledRequest {
  disabled: boolean
}

export interface SetPriorityRequest {
  priority: number
}

// Add credentialRequest
export interface AddCredentialRequest {
  refreshToken?: string
  accessToken?: string
  profileArn?: string
  expiresAt?: string
  authMethod?: 'social' | 'idc' | 'api_key'
  provider?: string
  clientId?: string
  clientSecret?: string
  startUrl?: string
  priority?: number
  authRegion?: string
  apiRegion?: string
  machineId?: string
  proxyUrl?: string
  proxyUsername?: string
  proxyPassword?: string
  kiroApiKey?: string
  endpoint?: string
  email?: string
  groups?: string[]
  sourceChannel?: string
}

// Add credentialresponse
export interface AddCredentialResponse {
  success: boolean
  message: string
  credentialId: number
  email?: string
}

// updateCredentialRequest(fieldas undefined means no change,emptystringrepresentsclear)
export interface UpdateCredentialRequest {
  email?: string
  proxyUrl?: string
  proxyUsername?: string
  proxyPassword?: string
  /** Accountbelongs toGroup(undefined means no change,an array represents the wholeReplace) */
  groups?: string[]
  /** Account source channel(undefined means no change,empty string meansclear) */
  sourceChannel?: string
}

// update refreshToken Request
export interface UpdateRefreshTokenRequest {
  refreshToken: string
  accessToken?: string
  expiresAt?: string
}

// ProxyhealthStatus
export type ProxyHealth = 'unknown' | 'healthy' | 'unhealthy'

// Proxy poolitemsitem
export interface ProxyPoolEntry {
  id: number
  url: string
  label?: string
  enabled: boolean
  credentialCount: number
  health: ProxyHealth
  latencyMs?: number
  lastCheckedAt?: string
  consecutiveFailures: number
  autoDisabled: boolean
}

// Proxy poolListresponse
export interface ProxyPoolResponse {
  total: number
  proxies: ProxyPoolEntry[]
}

// AddProxyRequest
export interface AddProxyRequest {
  url: string
  label?: string
}

// BulkAddProxyRequest
export interface BatchAddProxyRequest {
  urls: string[]
}

// assignProxygiveCredentialRequest
export interface AssignProxyRequest {
  proxyId?: number | null
}

// BulkAddProxyresponse
export interface BatchAddProxyResponse {
  added: number
  errors: number
  proxies: ProxyPoolEntry[]
  errorMessages: string[]
}

// singleproxieshealth check response
export interface ProxyCheckResponse {
  id: number
  health: ProxyHealth
  latencyMs?: number
  lastCheckedAt?: string
  enabled: boolean
  autoDisabled: boolean
}

// full health check response
export interface ProxyCheckAllResponse {
  healthy: number
  unhealthy: number
  autoDisabled: number
}

// pollBulkassignRequest
export interface AssignRoundRobinRequest {
  credentialIds?: number[] | null
}

// pollBulkassignment response
export interface AssignRoundRobinResponse {
  assigned: number
  proxyCount: number
}

// GlobalProxy config
export interface GlobalProxyResponse {
  proxyUrl: string | null
}

export interface SetGlobalProxyRequest {
  proxyUrl: string | null
}

// Online updateConfig
export interface UpdateConfigResponse {
  /** previoustimesbefore update it isinrunofVersion number(carry v prefix);storeincan at that timeCallsfallback endpoint */
  previousVersion?: string
  /** previoustimesSuccesscompleteOnline updateofTime(RFC3339) */
  lastAppliedAt?: string
  /** whetherConfigured GitHub Token(onlyBackboolean,does not return plaintext) */
  githubTokenSet: boolean
  /** whetherEnableUnattended auto-update */
  autoApply: boolean
  /** auto updateTrigger time (local time zone,HH:MM 24 hourscontrol) */
  autoApplyTime: string
}

export interface SetUpdateConfigRequest {
  /** GitHub Personal Access Token;emptystringrepresentsClear */
  githubToken?: string
  autoApply?: boolean
  autoApplyTime?: string
}

/** GitHub API Rate limitStatus(including token Validateresult) */
export interface GitHubRateLimitInfo {
  /** provideof token whetherhaseffect(none token timeas false but can still be foundAnonymousQuota) */
  valid: boolean
  /** whether it carries token Calls(false = Anonymousquery) */
  authenticated: boolean
  /** Rate limitlimit(Anonymous 60,authentication 5000) */
  limit: number
  /** RemainingAvailabletimescount */
  remaining: number
  /** Usedtimescount */
  used: number
  /** Rate limitwindowResetTime(Unix second) */
  reset: number
  /** token correspondingofUsername(maybeasempty) */
  login?: string
  /** Failedtimeofhint info */
  warning?: string
}

export interface ImageUpdateResponse {
  success: boolean
  message: string
  output?: string
  applied: boolean
  needRestart: boolean
}

export interface UpdateCheckInfo {
  currentVersion: string
  latestVersion: string
  hasUpdate: boolean
  buildType: string
  releaseName?: string
  releaseNotes?: string
  releaseUrl?: string
  publishedAt?: string
  checkedAt: string
  cached: boolean
  warning?: string
}

// LoginAPIKeymodify(adminApiKey —— ManagepanelLoginKey)
export interface UpdateAdminKeyRequest {
  newKey: string
}

// IdC Device authorizationLogin
export interface StartIdcLoginRequest {
  region: string
  startUrl?: string
  priority?: number
  email?: string
  proxyUrl?: string
}

export interface StartIdcLoginResponse {
  sessionId: string
  userCode: string
  verificationUri: string
  verificationUriComplete?: string
  expiresAt: string
  pollInterval: number
}

export type PollIdcLoginResponse =
  | { status: 'pending' }
  | { status: 'success'; credentialId: number }
  | { status: 'expired' }

// Social Login (Portal PKCE OAuth)
export interface StartSocialLoginRequest {
  priority?: number
  email?: string
  proxyUrl?: string
  authEndpoint?: string
  /** OAuth callback public address(remote mode),by API Client keybased on the current access addressAuto-derive,Callsusually no need to fill in */
  callbackBaseUrl?: string
}

/** complete manually on remote access Social Login:frombrowser address barPasteofcallback URL extract the parameter from */
export interface CompleteSocialLoginRequest {
  code: string
  state: string
  loginOption?: string
  path?: string
}

export interface StartSocialLoginResponse {
  sessionId: string
  portalUrl: string
  expiresAt: string
  /** whether in remote callback mode(serverConfigured callbackBaseUrl).
   *  true time OAuth callback points to a public pathby,the frontend can poll to complete automatically. */
  remote: boolean
}

export type PollSocialLoginResponse = PollIdcLoginResponse

// ============ Client key API Key distribute ============

export interface ClientKeyItem {
  id: number
  /** after maskingof Key(display only) */
  maskedKey: string
  name: string
  description?: string
  disabled: boolean
  createdAt: string
  lastUsedAt?: string
  totalCalls: number
  totalInputTokens: number
  totalOutputTokens: number
  totalCacheCreationTokens: number
  totalCacheReadTokens: number
  /** bindofAccount group(when not boundas undefined) */
  group?: string
  /** whetherSystemKey(config.json apiKey imported, cannot be deleted / Not rotatable) */
  isSystem: boolean
}

export interface ClientKeysResponse {
  total: number
  keys: ClientKeyItem[]
}

export interface CreateClientKeyRequest {
  name: string
  description?: string
  group?: string
}

/** Createresponse:plaintext Key only inhereBackonetimes */
export interface CreateClientKeyResponse {
  id: number
  key: string
  name: string
  createdAt: string
}

export interface UpdateClientKeyRequest {
  name?: string
  description?: string
  group?: string
}

// ============ Usagestatistics ============

export type StatsRange = '24h' | '7d' | '30d'
export type StatsGranularity = 'hour' | 'day'

export interface StatsTimeFilter {
  range?: StatsRange
  startDate?: string
  endDate?: string
  granularity: StatsGranularity
}

export interface StatsFilter {
  /** do not pass = All;other values = Client key id */
  keyId?: number
  /** byAccount groupfilter(only affects timeseries / by-credential,by-model notSupported) */
  group?: string
}

export interface OverviewStats {
  todayCalls: number
  todayInputTokens: number
  todayOutputTokens: number
  todayErrors: number
  todayCredits: number
  weekCalls: number
  weekInputTokens: number
  weekOutputTokens: number
  weekCredits: number
  activeClientKeys: number
  activeCredentials: number
}

export interface TimeSeriesPoint {
  ts: string
  inputTokens: number
  outputTokens: number
  cacheCreationTokens: number
  cacheReadTokens: number
  calls: number
  errors: number
  credits: number
}

export interface ModelDistribution {
  model: string
  calls: number
  inputTokens: number
  outputTokens: number
}

export interface CredentialDistribution {
  credentialId: number
  email?: string
  calls: number
  inputTokens: number
  outputTokens: number
  errors: number
}

// ============ Request trace ============

/** singletimesUpstreamtry */
export interface TraceAttempt {
  attempt: number
  credentialId: number
  email?: string | null
  endpoint: string
  /** Upstream HTTP Statuscode;null = network layerFailed */
  httpStatus: number | null
  /** success / quota_exhausted / account_throttled / auth_failed / transient / network_error / bad_request / unknown */
  outcome: string
  /** UpstreamErrorbody fragment(truncated) */
  errorSnippet: string | null
  durationMs: number
}

/** oneitemsexternalRequestoffull trace */
export interface TraceRecord {
  traceId: string
  ts: string
  keyId: number
  /** masterApiKey = history master Calls(offline);clientKey = Client key */
  keySource: 'masterApiKey' | 'clientKey'
  /** initiateRequestofClient key Name(master represents the primary apiKey;Managemember business Key canas null) */
  keyName?: string | null
  model: string
  isStream: boolean
  /** success / error / interrupted */
  finalStatus: string
  finalCredentialId: number
  finalEmail?: string | null
  errorType: string | null
  errorMessage: string | null
  totalAttempts: number
  durationMs: number
  /** StreamingInterruptedalready sent at that timebytescount */
  interruptedAfterBytes: number | null
  /** Input token */
  inputTokens?: number
  /** Output token */
  outputTokens?: number
  /** Cache create token */
  cacheCreationTokens?: number
  /** Cache read token */
  cacheReadTokens?: number
  /** total token = input + output + cache_creation + cache_read */
  totalTokens?: number
  /** Cost(credits) */
  credits?: number
  /** first Token delay(millisecond,onlyStreaminghasvalue) */
  firstTokenMs?: number | null
  attempts: TraceAttempt[]
}

/** trace query parameter */
export interface TraceQuery {
  status?: string
  errorType?: string
  credentialId?: number
  /** by initiatorRequestofClient key filter(0 = master apiKey) */
  keyId?: number
  /** theCredentialina certainjumpFailedpast(even if trace finalSuccess)——Used forCredentialFaileddetail */
  failedAttemptCredentialId?: number
  model?: string
  /** byAccount groupfilter by name(onlyBack final_credential_id belongs to thisGroupof trace) */
  group?: string
  onlyFailed?: boolean
  limit?: number
  offset?: number
}

/** splitpageresponse */
export interface TracePage {
  records: TraceRecord[]
  total: number
}

/** singleCredentialFailedcount by category(authenticate / Account throttle / other) */
export interface FailureStats {
  auth: number
  throttle: number
  other: number
}

/** credentialId(string) → Failedcount by category */
export type FailureStatsMap = Record<string, FailureStats>

// ============ Account group(independent entity)============

export interface GroupItem {
  name: string
  description?: string
  createdAt: string
  /** reference count:hashow manycredentialscarry thisgroups */
  credentialCount: number
  /** reference count:hashow manyput the client key bind thisgroups */
  clientKeyCount: number
}

export interface GroupsResponse {
  total: number
  groups: GroupItem[]
}

export interface CreateGroupRequest {
  name: string
  description?: string
}

export interface UpdateGroupRequest {
  /** new name;do not passorif same as the original name then notRename */
  newName?: string
  /** newNote;emptystringClear;undefined keep the original value */
  description?: string
}
