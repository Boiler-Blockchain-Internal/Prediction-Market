import { NextResponse } from 'next/server'
import { getSession } from '@/lib/auth/server'

export const runtime = 'nodejs'

export async function GET() {
  const session = await getSession()
  return NextResponse.json({ session }, { headers: { 'Cache-Control': 'no-store' } })
}
