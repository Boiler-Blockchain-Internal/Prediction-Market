import 'server-only'

import { createHash, randomBytes } from 'node:crypto'
import { cookies } from 'next/headers'
import { createSiweMessage, parseSiweMessage, validateSiweMessage } from 'viem/siwe'
import { createPublicClient, getAddress, http, type Address, type Hex } from 'viem'
import { sepolia } from 'viem/chains'

const SESSION_TTL_SECONDS = 60 * 60 * 24 * 7
const CHALLENGE_TTL_MS = 5 * 60 * 1000
const SUPPORTED_CHAIN_ID = sepolia.id
const SIGN_IN_STATEMENT = 'Sign in to BoilerMarket. This request will not submit a transaction.'
const SESSION_COOKIE = process.env.NODE_ENV === 'production' ? '__Host-bm_session' : 'bm_session'
const MAX_SIGNATURE_LENGTH = 20_002

type UserRow = { id: number }
type SessionRow = { wallet_address: string; expires_at: string }
type AuthPublicClient = ReturnType<typeof createPublicClient>

let cachedVerificationClient: AuthPublicClient | undefined

function appOrigin() {
  const configuredUrl = process.env.NEXT_PUBLIC_APP_URL
  if (!configuredUrl) throw new Error('NEXT_PUBLIC_APP_URL is required for SIWE authentication.')
  const url = new URL(configuredUrl)
  if (process.env.NODE_ENV === 'production' && url.protocol !== 'https:') {
    throw new Error('NEXT_PUBLIC_APP_URL must use HTTPS in production.')
  }
  return url
}

function supabaseConfig() {
  const url = process.env.SUPABASE_URL
  const secretKey = process.env.SUPABASE_SECRET_KEY
  if (!url || !secretKey) throw new Error('SUPABASE_URL and SUPABASE_SECRET_KEY are required for wallet authentication.')
  return { url: url.replace(/\/+$/, ''), secretKey }
}

function getVerificationClient() {
  if (cachedVerificationClient) return cachedVerificationClient

  const rpcUrl = process.env.SEPOLIA_RPC_URL
  if (!rpcUrl) throw new Error('SEPOLIA_RPC_URL is required to verify wallet signatures.')

  const parsedRpcUrl = new URL(rpcUrl)
  if (parsedRpcUrl.protocol !== 'https:' && !(process.env.NODE_ENV !== 'production' && parsedRpcUrl.protocol === 'http:')) {
    throw new Error('SEPOLIA_RPC_URL must use HTTPS outside local development.')
  }

  cachedVerificationClient = createPublicClient({
    chain: sepolia,
    transport: http(rpcUrl),
  })
  return cachedVerificationClient
}

async function supabaseRequest<T>(path: string, init: RequestInit = {}): Promise<T> {
  const { url, secretKey } = supabaseConfig()
  const headers = new Headers(init.headers)
  headers.set('apikey', secretKey)
  // Legacy service-role keys are JWTs and also need Authorization. New sb_secret
  // keys are API keys, not JWTs, and should be sent only in the apikey header.
  if (secretKey.startsWith('eyJ')) headers.set('Authorization', `Bearer ${secretKey}`)
  if (init.body) headers.set('Content-Type', 'application/json')

  const response = await fetch(`${url}/rest/v1/${path}`, {
    ...init,
    headers,
    cache: 'no-store',
  })
  if (!response.ok) throw new Error(`Supabase auth storage returned ${response.status}.`)
  const responseBody = await response.text()
  return responseBody ? (JSON.parse(responseBody) as T) : (undefined as T)
}

function hash(value: string) {
  return createHash('sha256').update(value).digest('hex')
}

export function hasTrustedOrigin(request: Request) {
  const origin = request.headers.get('origin')
  return origin === appOrigin().origin
}

export async function createChallenge(addressInput: string, chainId: number) {
  if (chainId !== SUPPORTED_CHAIN_ID) throw new Error('Unsupported network. Switch to Sepolia and try again.')
  const address = getAddress(addressInput)
  const origin = appOrigin()
  const nonce = randomBytes(16).toString('hex')
  const expiresAt = new Date(Date.now() + CHALLENGE_TTL_MS)
  const message = createSiweMessage({
    domain: origin.host,
    address,
    statement: SIGN_IN_STATEMENT,
    uri: origin.origin,
    version: '1',
    chainId,
    nonce,
    issuedAt: new Date(),
    expirationTime: expiresAt,
  })

  await supabaseRequest('wallet_auth_challenges', {
    method: 'POST',
    headers: { Prefer: 'return=minimal' },
    body: JSON.stringify({
      nonce_hash: hash(nonce),
      wallet_address: address.toLowerCase(),
      chain_id: chainId,
      expires_at: expiresAt.toISOString(),
    }),
  })
  return { message, expiresInSeconds: CHALLENGE_TTL_MS / 1000 }
}

async function consumeChallenge(nonce: string, address: Address, chainId: number) {
  const now = new Date().toISOString()
  const query = new URLSearchParams({
    nonce_hash: `eq.${hash(nonce)}`,
    wallet_address: `eq.${address.toLowerCase()}`,
    chain_id: `eq.${chainId}`,
    consumed_at: 'is.null',
    expires_at: `gt.${now}`,
    select: 'nonce_hash',
  })
  const consumed = await supabaseRequest<Array<{ nonce_hash: string }>>(`wallet_auth_challenges?${query}`, {
    method: 'PATCH',
    headers: { Prefer: 'return=representation' },
    body: JSON.stringify({ consumed_at: now }),
  })
  return consumed.length === 1
}

export async function verifySiweLogin(message: string, signature: string) {
  const origin = appOrigin()
  const fields = parseSiweMessage(message)
  if (!fields.address || !fields.nonce || !fields.chainId) throw new Error('Malformed sign-in message.')
  if (fields.chainId !== SUPPORTED_CHAIN_ID || fields.uri !== origin.origin || fields.statement !== SIGN_IN_STATEMENT) {
    throw new Error('The sign-in message does not match BoilerMarket or the supported network.')
  }
  const address = getAddress(fields.address)
  const isValidMessage = validateSiweMessage({
    message: fields,
    address,
    domain: origin.host,
    nonce: fields.nonce,
    time: new Date(),
  })
  if (!isValidMessage) throw new Error('The sign-in message is invalid or expired. Please try again.')
  if (
    signature.length > MAX_SIGNATURE_LENGTH ||
    !/^0x(?:[0-9a-fA-F]{2})+$/.test(signature)
  ) {
    throw new Error('Invalid wallet signature.')
  }

  let validSignature: boolean
  try {
    // The public-client action checks both EOA signatures and deployed contract
    // wallets through ERC-1271 on the SIWE message's supported chain.
    validSignature = await getVerificationClient().verifyMessage({
      address,
      message,
      signature: signature as Hex,
    })
  } catch {
    throw new Error('Could not verify the wallet signature on Sepolia. Please try again.')
  }
  if (!validSignature) throw new Error('The wallet signature could not be verified.')
  if (!(await consumeChallenge(fields.nonce, address, fields.chainId))) {
    throw new Error('This sign-in request expired or was already used. Please try again.')
  }
  return address
}

async function getOrCreateUser(address: Address) {
  const walletAddress = address.toLowerCase()
  const query = new URLSearchParams({
    on_conflict: 'wallet_address',
    select: 'id',
  })
  await supabaseRequest<UserRow[]>(`users?${query}`, {
    method: 'POST',
    headers: { Prefer: 'resolution=ignore-duplicates,return=representation' },
    body: JSON.stringify({ wallet_address: walletAddress }),
  })

  const lookup = new URLSearchParams({
    wallet_address: `eq.${walletAddress}`,
    select: 'id',
    limit: '1',
  })
  const users = await supabaseRequest<UserRow[]>(`users?${lookup}`)
  if (!users[0]) throw new Error('Could not find or create the wallet profile.')
  return users[0].id
}

export async function createSession(address: Address) {
  const userId = await getOrCreateUser(address)
  const id = randomBytes(32).toString('hex')
  const expiresAt = new Date(Date.now() + SESSION_TTL_SECONDS * 1000)
  await supabaseRequest('wallet_auth_sessions', {
    method: 'POST',
    headers: { Prefer: 'return=minimal' },
    body: JSON.stringify({
      id_hash: hash(id),
      user_id: userId,
      wallet_address: address.toLowerCase(),
      expires_at: expiresAt.toISOString(),
    }),
  })
  return { id, expiresAt: expiresAt.getTime() }
}

export async function getSession() {
  const cookieStore = await cookies()
  const id = cookieStore.get(SESSION_COOKIE)?.value
  if (!id) return null
  const now = new Date().toISOString()
  const query = new URLSearchParams({
    id_hash: `eq.${hash(id)}`,
    revoked_at: 'is.null',
    expires_at: `gt.${now}`,
    select: 'wallet_address,expires_at',
    limit: '1',
  })
  const rows = await supabaseRequest<SessionRow[]>(`wallet_auth_sessions?${query}`)
  const session = rows[0]
  return session ? { address: session.wallet_address, expiresAt: new Date(session.expires_at).getTime() } : null
}

export async function revokeCurrentSession() {
  const cookieStore = await cookies()
  const id = cookieStore.get(SESSION_COOKIE)?.value
  if (!id) return
  const query = new URLSearchParams({ id_hash: `eq.${hash(id)}`, revoked_at: 'is.null' })
  await supabaseRequest(`wallet_auth_sessions?${query}`, {
    method: 'PATCH',
    headers: { Prefer: 'return=minimal' },
    body: JSON.stringify({ revoked_at: new Date().toISOString() }),
  })
}

export function sessionCookieOptions() {
  return {
    name: SESSION_COOKIE,
    httpOnly: true,
    secure: process.env.NODE_ENV === 'production',
    sameSite: 'lax' as const,
    path: '/',
    maxAge: SESSION_TTL_SECONDS,
  }
}

export function clearedSessionCookieOptions() {
  return { ...sessionCookieOptions(), maxAge: 0, expires: new Date(0) }
}
