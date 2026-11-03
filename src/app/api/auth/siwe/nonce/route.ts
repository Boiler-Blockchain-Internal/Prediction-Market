import { NextResponse } from 'next/server'
import { createChallenge, hasTrustedOrigin } from '@/lib/auth/server'

export const runtime = 'nodejs'

export async function POST(request: Request) {
  if (!hasTrustedOrigin(request)) return NextResponse.json({ error: 'Request origin is not allowed.' }, { status: 403 })
  if (Number(request.headers.get('content-length') ?? 0) > 2048) {
    return NextResponse.json({ error: 'Request body is too large.' }, { status: 413 })
  }
  try {
    const body = (await request.json()) as { address?: unknown; chainId?: unknown }
    if (typeof body.address !== 'string' || typeof body.chainId !== 'number') {
      return NextResponse.json({ error: 'A wallet address and chain ID are required.' }, { status: 400 })
    }
    const challenge = await createChallenge(body.address, body.chainId)
    return NextResponse.json(challenge, { headers: { 'Cache-Control': 'no-store' } })
  } catch (error) {
    const message = error instanceof Error ? error.message : 'Could not create a sign-in request.'
    const status = message.includes('Unsupported network') ? 400 : 500
    return NextResponse.json({ error: message }, { status })
  }
}
