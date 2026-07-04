const API_KEY_STORAGE_KEY = 'adminApiKey'
const CREDENTIAL_VIEW_KEY = 'credentialView'
const CREDENTIAL_PAGE_SIZE_KEY = 'credentialPageSize'

export type CredentialView = 'card' | 'list'

/** Per pageCount:0 viewas“All”(no splitpage) */
const DEFAULT_PAGE_SIZE = 12

export const storage = {
  getApiKey: () => localStorage.getItem(API_KEY_STORAGE_KEY),
  setApiKey: (key: string) => localStorage.setItem(API_KEY_STORAGE_KEY, key),
  removeApiKey: () => localStorage.removeItem(API_KEY_STORAGE_KEY),

  // Credential listofdisplay form(Card / List),DefaultCard
  getCredentialView: (): CredentialView =>
    localStorage.getItem(CREDENTIAL_VIEW_KEY) === 'list' ? 'list' : 'card',
  setCredentialView: (view: CredentialView) =>
    localStorage.setItem(CREDENTIAL_VIEW_KEY, view),

  // Credential listPer pageCount(0 = All),Default 12
  getCredentialPageSize: (): number => {
    const raw = localStorage.getItem(CREDENTIAL_PAGE_SIZE_KEY)
    if (raw === null) return DEFAULT_PAGE_SIZE
    const n = Number(raw)
    return Number.isInteger(n) && n >= 0 ? n : DEFAULT_PAGE_SIZE
  },
  setCredentialPageSize: (size: number) =>
    localStorage.setItem(CREDENTIAL_PAGE_SIZE_KEY, String(size)),
}
