import { cookieStorage, createStorage } from 'wagmi'
import { WagmiAdapter } from '@reown/appkit-adapter-wagmi'
import { sepolia } from '@reown/appkit/networks'

export const projectId = process.env.NEXT_PUBLIC_REOWN_PROJECT_ID

// Sepolia is a provisional test network for connection work only. Confirm the
// product's target chain before expanding this list or deploying the app.
export const networks = [sepolia] as const

export const wagmiAdapter = projectId
  ? new WagmiAdapter({
      projectId,
      networks: [...networks],
      ssr: true,
      storage: createStorage({ storage: cookieStorage }),
    })
  : null
