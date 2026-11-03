
-- 001_initial_schema
-- Development schema for Prediction Market

CREATE TABLE public.users (
    id BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    wallet_address TEXT NOT NULL UNIQUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE public.markets (
    id BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    onchain_market_id TEXT NOT NULL UNIQUE,
    question TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'open'
        CHECK (status IN ('open', 'resolved', 'voided')),
    outcome TEXT,
    closes_at TIMESTAMPTZ,
    resolved_at TIMESTAMPTZ,
    CHECK (outcome IS NULL OR outcome IN ('YES', 'NO')),
    CHECK (
        (status = 'resolved' AND outcome IS NOT NULL)
        OR (status <> 'resolved' AND outcome IS NULL)
    )
);

CREATE TABLE public.trades (
    id BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    user_id BIGINT NOT NULL REFERENCES public.users(id),
    market_id BIGINT NOT NULL REFERENCES public.markets(id),

    action TEXT NOT NULL CHECK (action IN ('buy', 'sell')),
    role TEXT NOT NULL CHECK (role IN ('maker', 'taker')),
    side TEXT NOT NULL CHECK (side IN ('YES', 'NO')),

    shares NUMERIC NOT NULL CHECK (shares > 0),
    price NUMERIC NOT NULL CHECK (price > 0 AND price < 1),
    fee NUMERIC NOT NULL DEFAULT 0 CHECK (fee >= 0),

    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    block_number BIGINT,
    tx_hash TEXT,
    log_index INTEGER,

    UNIQUE (tx_hash, log_index, role)
);

CREATE TABLE public.reputation_snapshots (
    user_id BIGINT NOT NULL REFERENCES public.users(id),
    as_of TIMESTAMPTZ NOT NULL,
    method_version TEXT NOT NULL,

    n_positions INTEGER NOT NULL DEFAULT 0,
    score NUMERIC CHECK (score BETWEEN 0 AND 100),
    win_rate NUMERIC CHECK (win_rate BETWEEN 0 AND 1),

    -- Reputation team metrics
    realized_edge NUMERIC,
    performance_score NUMERIC,
    consistency_score NUMERIC,
    confidence_score NUMERIC,

    PRIMARY KEY (user_id, as_of, method_version)
);

-- Helpful indexes for processing trades
CREATE INDEX idx_trades_user_market_time
ON public.trades (user_id, market_id, created_at, id);

CREATE INDEX idx_reputation_snapshots_score
ON public.reputation_snapshots (score DESC);

-- Secure all four tables until access policies are finalized.
ALTER TABLE public.users ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.markets ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.trades ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.reputation_snapshots ENABLE ROW LEVEL SECURITY;

REVOKE ALL ON TABLE
    public.users,
    public.markets,
    public.trades,
    public.reputation_snapshots
FROM anon, authenticated;

REVOKE ALL ON ALL SEQUENCES IN SCHEMA public
FROM anon, authenticated;
