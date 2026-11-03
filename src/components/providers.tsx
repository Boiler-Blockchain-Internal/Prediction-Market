'use client'

import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { createAppKit } from '@reown/appkit/react'
import { cookieToInitialState, WagmiProvider, type Config } from 'wagmi'
import type { ReactNode } from 'react'
import { networks, projectId, wagmiAdapter } from '@/lib/wallet/config'

const queryClient = new QueryClient()

if (projectId && wagmiAdapter) {
  createAppKit({
    adapters: [wagmiAdapter],
    projectId,
    networks: [...networks],
    defaultNetwork: networks[0],
    metadata: {
      name: 'BoilerMarket',
      description: 'A social forecasting platform',
      url: process.env.NEXT_PUBLIC_APP_URL ?? 'http://localhost:3000',
      icons: [],
    },
    features: { analytics: false },
  })
}

export function Providers({ children, cookies }: { children: ReactNode; cookies: string | null }) {
  if (!projectId || !wagmiAdapter) return children

  const initialState = cookieToInitialState(wagmiAdapter.wagmiConfig as Config, cookies)

  return (
    <WagmiProvider config={wagmiAdapter.wagmiConfig as Config} initialState={initialState}>
      <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
    </WagmiProvider>
  )
}
