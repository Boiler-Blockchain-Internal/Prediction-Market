import type { Metadata } from 'next'
import { headers } from 'next/headers'
import { Providers } from '@/components/providers'
import './globals.css'

export const metadata: Metadata = {
  title: 'BoilerMarket',
  description: 'A social forecasting platform',
}

export default async function RootLayout({ children }: Readonly<{ children: React.ReactNode }>) {
  const cookies = (await headers()).get('cookie')

  return (
    <html lang="en">
      <body>
        <Providers cookies={cookies}>{children}</Providers>
      </body>
    </html>
  )
}
