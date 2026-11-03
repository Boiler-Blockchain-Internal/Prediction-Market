import { NextResponse } from 'next/server'
import { clearedSessionCookieOptions, hasTrustedOrigin, revokeCurrentSession } from '@/lib/auth/server'

export const runtime = 'nodejs'

export async function POST(request: Request) {
  if (!hasTrustedOrigin(request)) return NextResponse.json({ error: 'Request origin is not allowed.' }, { status: 403 })
  await revokeCurrentSession()
  const response = NextResponse.json({ ok: true })
  const cookie = clearedSessionCookieOptions()
  response.cookies.set(cookie.name, '', cookie)
  return response
}
