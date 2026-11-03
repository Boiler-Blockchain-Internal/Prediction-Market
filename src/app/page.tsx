import { WalletStatus } from '@/components/wallet-status'

export default function Home() {
  return (
    <main className="page-shell">
      <section className="hero">
        <p className="eyebrow">Boiler Blockchain</p>
        <h1>BoilerMarket</h1>
        <p className="intro">A social forecasting platform where predictions build reputation.</p>
        <WalletStatus />
        <p className="scope-note">
          Wallet sign-in uses a signed message and does not submit a transaction. Prediction and transaction features are not
          included in this starter.
        </p>
      </section>
    </main>
  )
}
