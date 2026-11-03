import { NextResponse } from 'next/server'
import { createSession, hasTrustedOrigin, sessionCookieOptions, verifySiweLogin } from '@/lib/auth/server'

export const runtime = 'nodejs'

export async function POST(request: Request) {
  if (!hasTrustedOrigin(request)) return NextResponse.json({ error: 'Request origin is not allowed.' }, { status: 403 })
  if (Number(request.headers.get('content-length') ?? 0) > 12_000) {
    return NextResponse.json({ error: 'Request body is too large.' }, { status: 413 })
  }
  try {
    const body = (await request.json()) as { message?: unknown; signature?: unknown }
    if (typeof body.message !== 'string' || typeof body.signature !== 'string') {
      return NextResponse.json({ error: 'A sign-in message and signature are required.' }, { status: 400 })
    }
    const address = await verifySiweLogin(body.message, body.signature)
    const session = await createSession(address)
    const response = NextResponse.json({ address, expiresAt: session.expiresAt }, { headers: { 'Cache-Control': 'no-store' } })
    const cookie = sessionCookieOptions()
    response.cookies.set(cookie.name, session.id, cookie)
    return response
  } catch (error) {
    const message = error instanceof Error ? error.message : 'Could not verify this wallet sign-in.'
    return NextResponse.json({ error: message }, { status: 401 })
  }
}
