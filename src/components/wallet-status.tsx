'use client'

import { useCallback, useEffect, useState } from 'react'
import { useAccount, useSignMessage } from 'wagmi'
import { projectId } from '@/lib/wallet/config'

type AuthSession = { address: string; expiresAt: number } | null

function ConnectedWalletStatus() {
  const { address, chain, isConnected } = useAccount()
  const { signMessageAsync, isPending } = useSignMessage()
  const [session, setSession] = useState<AuthSession>(null)
  const [loadingSession, setLoadingSession] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [working, setWorking] = useState(false)

  const refreshSession = useCallback(async () => {
    try {
      const response = await fetch('/api/auth/session', { cache: 'no-store' })
      const result = (await response.json()) as { session: AuthSession }
      setSession(result.session)
    } catch {
      setSession(null)
    } finally {
      setLoadingSession(false)
    }
  }, [])

  useEffect(() => {
    let active = true
    fetch('/api/auth/session', { cache: 'no-store' })
      .then((response) => response.json() as Promise<{ session: AuthSession }>)
      .then((result) => { if (active) setSession(result.session) })
      .catch(() => { if (active) setSession(null) })
      .finally(() => { if (active) setLoadingSession(false) })
    return () => { active = false }
  }, [])

  async function signIn() {
    if (!address || !chain) return
    setWorking(true)
    setError(null)
    try {
      const challengeResponse = await fetch('/api/auth/siwe/nonce', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ address, chainId: chain.id }),
      })
      const challenge = (await challengeResponse.json()) as { message?: string; error?: string }
      if (!challengeResponse.ok || !challenge.message) throw new Error(challenge.error ?? 'Could not start sign-in.')
      const signature = await signMessageAsync({ message: challenge.message })
      const verifyResponse = await fetch('/api/auth/siwe/verify', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ message: challenge.message, signature }),
      })
      const result = (await verifyResponse.json()) as { address?: string; error?: string }
      if (!verifyResponse.ok || !result.address) throw new Error(result.error ?? 'Could not verify wallet sign-in.')
      await refreshSession()
    } catch (cause) {
      const message = cause instanceof Error ? cause.message : 'Wallet sign-in failed. Please try again.'
      setError(message.includes('User rejected') ? 'Signature request cancelled. You are still connected.' : message)
    } finally {
      setWorking(false)
    }
  }

  async function signOut() {
    setError(null)
    try {
      const response = await fetch('/api/auth/logout', { method: 'POST' })
      if (!response.ok) throw new Error('Could not sign out. Please try again.')
      setSession(null)
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : 'Could not sign out.')
    }
  }

  const signedInForConnectedWallet = Boolean(session && address && session.address.toLowerCase() === address.toLowerCase())
  const sessionForDifferentWallet = Boolean(session && address && !signedInForConnectedWallet)

  return (
    <div className="wallet-panel">
      <appkit-button />
      {isConnected && address ? (
        <>
          <p className="wallet-detail">
            Connected: <code>{address}</code>{chain ? ` on ${chain.name}` : ''}
          </p>
          {loadingSession ? <p className="wallet-detail">Checking sign-in…</p> : signedInForConnectedWallet ? (
            <div className="auth-status" role="status">
              <p className="wallet-detail">Signed in as <code>{address}</code>.</p>
              <button className="auth-button secondary" onClick={signOut}>Sign out</button>
            </div>
          ) : sessionForDifferentWallet ? (
            <div className="auth-status">
              <p className="wallet-detail">A different wallet is signed in. Sign out before switching accounts.</p>
              <button className="auth-button secondary" onClick={signOut}>Sign out</button>
            </div>
          ) : (
            <button className="auth-button" onClick={signIn} disabled={working || isPending}>
              {working || isPending ? 'Signing in…' : 'Sign in with wallet'}
            </button>
          )}
        </>
      ) : (
        <>
          <p className="wallet-detail">Connect a wallet to continue. Connecting does not sign you in.</p>
          {!loadingSession && session ? (
            <div className="auth-status" role="status">
              <p className="wallet-detail">Signed in as <code>{session.address}</code>.</p>
              <button className="auth-button secondary" onClick={signOut}>Sign out</button>
            </div>
          ) : null}
        </>
      )}
      {error ? <p className="auth-error" role="alert">{error}</p> : null}
    </div>
  )
}

export function WalletStatus() {
  if (!projectId) {
    return (
      <div className="setup-note" role="status">
        Add <code>NEXT_PUBLIC_REOWN_PROJECT_ID</code> to <code>.env.local</code> to enable wallet connection.
      </div>
    )
  }

  return <ConnectedWalletStatus />
}
