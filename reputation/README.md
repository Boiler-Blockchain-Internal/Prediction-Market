# Prediction Market Reputation System

Python reputation scoring system for the Boiler Blockchain Prediction Market.

## Overview

The system reads trades from Supabase, reconstructs open and closed positions, calculates reputation metrics, and saves reputation snapshots.

## Scoring Metrics

- Realized edge: exit price minus average entry price.
- Performance score: normalized realized edge.
- Consistency score: percentage of profitable closed quantities.
- Confidence score: evidence based on trading history.
- Time decay: recent trades receive greater weight.

Current reputation weights (provisional):
- Performance: 60%
- Consistency: 25%
- Confidence: 15%

## Files

- `positions.py`: Reconstruct positions and handle settlement.
- `scoring.py`: Calculate realized edge and decay.
- `reputation.py`: Calculate reputation scores.
- `database.py`: Supabase connection and database operations.
- `trade_adapter.py`: Convert database trades into Python objects.
- `run_reputation.py`: Execute the scoring pipeline.
- `simulate_trades.py`: Demonstration with example trades.
- `test_reputation.py`: Unit tests.
- `migrations/`: Development database schema.

## Setup

Install dependencies:

    python -m pip install -r requirements.txt

Create a local `.env` file containing:

    SUPABASE_URL=<your-project-url>
    SUPABASE_SECRET_KEY=<your-secret-key>

Never commit `.env` or expose secret keys.

## Usage

Run unit tests:

    python -m unittest discover -v

Test database connection:

    python database.py

Calculate and save reputation:

    python run_reputation.py

## Development Status

Prototype integration complete using simulated trades.

Remaining work includes shared backend integration, scoring formula approval, production access policies, and leaderboard functionality.
