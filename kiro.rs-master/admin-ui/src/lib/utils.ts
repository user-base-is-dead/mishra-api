import { clsx, type ClassValue } from 'clsx'
import { twMerge } from 'tailwind-merge'

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs))
}

/**
 * parse the backendErrorresponse,extract a user friendlyofErrorinfo
 */
export interface ParsedError {
  /** shortofErrortitle */
  title: string
  /** detailedofErrorDescription */
  detail?: string
  /** Error type */
  type?: string
}

/**
 * fromErrorextract from the objectErrorMessage
 * Supported Axios Errorandnormal Error object
 */
export function extractErrorMessage(error: unknown): string {
  const parsed = parseError(error)
  return parsed.title
}

/**
 * OverageOperation failedhint:403 / Insufficient permissions uniform prompt to contact the organizationManagemember
 * (Enterprise / restricted by organization policyaccountscannot on its ownEnable overage)
 */
export function overageFailureMessage(raw?: string): string {
  const msg = (raw ?? '').trim()
  if (!msg) return 'Operation failed'
  if (/\b403\b|Forbidden|Insufficient permissions/i.test(msg)) {
    return 'Please contact your organization administrator for support'
  }
  return msg
}

/**
 * parseError,BackStructureconvertofErrorinfo
 */
export function parseError(error: unknown): ParsedError {
  if (!error || typeof error !== 'object') {
    return { title: 'Unknown error' }
  }

  const axiosError = error as Record<string, unknown>
  const response = axiosError.response as Record<string, unknown> | undefined
  const data = response?.data as Record<string, unknown> | undefined
  const errorObj = data?.error as Record<string, unknown> | undefined

  // tryfrombackendErrorextract info from the response
  if (errorObj && typeof errorObj.message === 'string') {
    const message = errorObj.message
    const type = typeof errorObj.type === 'string' ? errorObj.type : undefined

    // parse nestedofErrorinfo(such as:UpstreamService error: Insufficient permissions: 403 {...})
    const parsed = parseNestedErrorMessage(message)

    return {
      title: parsed.title,
      detail: parsed.detail,
      type,
    }
  }

  // Roll back to Error.message
  if ('message' in axiosError && typeof axiosError.message === 'string') {
    return { title: axiosError.message }
  }

  return { title: 'Unknown error' }
}

/**
 * parse nestedofErrorMessage
 * For example:"UpstreamService error: Insufficient permissions,cannot obtainUseQuota: 403 Forbidden {...}"
 */
function parseNestedErrorMessage(message: string): { title: string; detail?: string } {
  // try to extract HTTP Statuscode(such as 403,502 etc.)
  const statusMatch = message.match(/(\d{3})\s+\w+/)
  const statusCode = statusMatch ? statusMatch[1] : null

  // try to extract JSON inof message field
  const jsonMatch = message.match(/\{[^{}]*"message"\s*:\s*"([^"]+)"[^{}]*\}/)
  if (jsonMatch) {
    const innerMessage = jsonMatch[1]
    // Extract the main error cause (drop the prefix)
    const parts = message.split(':').map(s => s.trim())
    const mainReason = parts.length > 1 ? parts[1].split(':')[0] : parts[0]

    // in title contains a status code
    const title = statusCode
      ? `${mainReason || 'Service error'} (${statusCode})`
      : (mainReason || 'Service error')

    return {
      title,
      detail: innerMessage,
    }
  }

  // Try splitting by colon to extract the main message
  const colonParts = message.split(':')
  if (colonParts.length >= 2) {
    const mainPart = colonParts[1].trim().split(':')[0].trim()
    const title = statusCode ? `${mainPart} (${statusCode})` : mainPart

    return {
      title,
      detail: colonParts.slice(2).join(':').trim() || undefined,
    }
  }

  return { title: message }
}




/**
 * Compact display of count semantics (K / M / B).
 *
 * Rule:< 1000 output as-is;≥ 1000 Use Intl of compact notation, keep at most 1 decimal places (such as 1.2K / 3.4M / 5.6B).
 * Used only for"Count / Amount / Size"semantics;ID / Port number / Version number / Page number / status code, do not use.
 */
export function formatNumber(value: number | null | undefined): string {
  if (value == null || Number.isNaN(value)) return '0'
  if (Math.abs(value) < 1000) return String(value)
  return new Intl.NumberFormat('en-US', {
    notation: 'compact',
    maximumFractionDigits: 1,
  }).format(value)
}

/**
 * Credit Billing amount display: upstream meteringEvent.usage is a float (such as 0.0169543),
 * unit is "credit". Uniformly keep 3 decimal places;≥ 1000 goes through when K/M/B Compact mode (compact
 * notation built-in 1 decimal places rounded, for example 1,234 → "1.2K").
 */
export function formatCredits(value: number | null | undefined): string {
  if (value == null || Number.isNaN(value) || value <= 0) return '0'
  if (value >= 1000) {
    return new Intl.NumberFormat('en-US', {
      notation: 'compact',
      maximumFractionDigits: 1,
    }).format(value)
  }
  return value.toFixed(3)
}

/**
 * Masked proxy URL: will user:pass@host replace the auth info in with xxx****xxx
 */
export function maskProxyUrl(url: string): string {
  const match = url.match(/^(\w+:\/\/)([^:@]+):([^@]+)@(.+)$/)
  if (!match) return url
  const [, scheme, user, pass, host] = match
  const mask = (s: string) =>
    s.length <= 6 ? '****' : `${s.slice(0, 3)}****${s.slice(-3)}`
  return `${scheme}${mask(user)}:${mask(pass)}@${host}`
}

/**
 * compute the string SHA-256 hash (hexadecimal)
 *
 * Prefer Web Crypto API(crypto.subtle), in a non-secure context (HTTP + not localhost) in
 * automatically fall back to plain JS implementation, resolving Docker when deploying crypto.subtle unavailable issue.
 */
export async function sha256Hex(value: string): Promise<string> {
  const encoded = new TextEncoder().encode(value)

  // use the native one in a secure context Web Crypto API(better performance)
  if (typeof crypto !== 'undefined' && crypto.subtle) {
    const digest = await crypto.subtle.digest('SHA-256', encoded)
    const bytes = new Uint8Array(digest)
    return Array.from(bytes).map(b => b.toString(16).padStart(2, '0')).join('')
  }

  // Non-secure context fallback: plain JS SHA-256 Implementation
  return sha256Pure(encoded)
}

/**
 * plain JS SHA-256 implementation (no external dependencies)
 * only in crypto.subtle used when unavailable
 */
function sha256Pure(data: Uint8Array): string {
  // SHA-256 constant: the first 64 the fractional part of the cube roots of prime numbers
  const K = new Uint32Array([
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
  ])

  const rotr = (x: number, n: number) => (x >>> n) | (x << (32 - n))

  // Preprocessing: pad the message
  const bitLen = data.length * 8
  // Message + 1 bytes 0x80 + Padding + 8 byte length, with total length aligned to 64 bytes
  const padLen = (((data.length + 9 + 63) >>> 6) << 6)
  const buf = new Uint8Array(padLen)
  buf.set(data)
  buf[data.length] = 0x80
  // Write 64 bit big-endian length (only the low 32 bits, high 32 bit in JS is always in 0)
  const view = new DataView(buf.buffer)
  view.setUint32(padLen - 4, bitLen, false)

  // initial hash value
  let h0 = 0x6a09e667, h1 = 0xbb67ae85, h2 = 0x3c6ef372, h3 = 0xa54ff53a
  let h4 = 0x510e527f, h5 = 0x9b05688c, h6 = 0x1f83d9ab, h7 = 0x5be0cd19

  const w = new Uint32Array(64)

  for (let offset = 0; offset < padLen; offset += 64) {
    for (let i = 0; i < 16; i++) {
      w[i] = view.getUint32(offset + i * 4, false)
    }
    for (let i = 16; i < 64; i++) {
      const s0 = rotr(w[i - 15], 7) ^ rotr(w[i - 15], 18) ^ (w[i - 15] >>> 3)
      const s1 = rotr(w[i - 2], 17) ^ rotr(w[i - 2], 19) ^ (w[i - 2] >>> 10)
      w[i] = (w[i - 16] + s0 + w[i - 7] + s1) | 0
    }

    let a = h0, b = h1, c = h2, d = h3, e = h4, f = h5, g = h6, h = h7

    for (let i = 0; i < 64; i++) {
      const S1 = rotr(e, 6) ^ rotr(e, 11) ^ rotr(e, 25)
      const ch = (e & f) ^ (~e & g)
      const temp1 = (h + S1 + ch + K[i] + w[i]) | 0
      const S0 = rotr(a, 2) ^ rotr(a, 13) ^ rotr(a, 22)
      const maj = (a & b) ^ (a & c) ^ (b & c)
      const temp2 = (S0 + maj) | 0

      h = g; g = f; f = e; e = (d + temp1) | 0
      d = c; c = b; b = a; a = (temp1 + temp2) | 0
    }

    h0 = (h0 + a) | 0; h1 = (h1 + b) | 0; h2 = (h2 + c) | 0; h3 = (h3 + d) | 0
    h4 = (h4 + e) | 0; h5 = (h5 + f) | 0; h6 = (h6 + g) | 0; h7 = (h7 + h) | 0
  }

  return [h0, h1, h2, h3, h4, h5, h6, h7]
    .map(v => (v >>> 0).toString(16).padStart(8, '0'))
    .join('')
}

/**
 * Generate a cryptographically strong random API Key
 *
 * Default 32 character random part (letters only, upper and lower case + digits,~190 bit entropy), plus `sk-kiro-` prefix;
 * Do not use `-` / `_`, to avoid being adjacent to the hyphen in the prefix `--`.
 * hard dependency `crypto.getRandomValues`, throwing an error when missing without any weak-entropy fallback fallback.
 */
export function generateApiKey(prefix: string = 'sk-kiro-', randomLen: number = 32): string {
  if (typeof crypto === 'undefined' || typeof crypto.getRandomValues !== 'function') {
    throw new Error('crypto.getRandomValues unavailable, cannot generate securely API Key')
  }
  const ALPHABET = 'abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789'
  // use rejection sampling to map bytes evenly onto 62 character table to avoid modulo bias (248 = 4 * 62)
  let out = ''
  const buf = new Uint8Array(randomLen)
  while (out.length < randomLen) {
    crypto.getRandomValues(buf)
    for (let i = 0; i < buf.length && out.length < randomLen; i++) {
      const b = buf[i]
      if (b < 248) out += ALPHABET[b % ALPHABET.length]
    }
  }
  return prefix + out
}

