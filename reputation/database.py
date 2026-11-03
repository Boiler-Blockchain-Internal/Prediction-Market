
import os
from dotenv import load_dotenv
from supabase import create_client

# Load credentials from .env
load_dotenv()

SUPABASE_URL = os.getenv("SUPABASE_URL")
SUPABASE_KEY = os.getenv("SUPABASE_SECRET_KEY")

if not SUPABASE_URL or not SUPABASE_KEY:
    raise ValueError(
        "Missing SUPABASE_URL or SUPABASE_SECRET_KEY in .env"
    )

# Create Supabase client
supabase = create_client(SUPABASE_URL, SUPABASE_KEY)


def fetch_trades():
    """Fetch trade records from Supabase."""
    response = (
        supabase.table("trades")
        .select("*")
        .execute()
    )
    return response.data


def fetch_markets():
    """Fetch market records from Supabase."""
    response = (
        supabase.table("markets")
        .select("*")
        .execute()
    )
    return response.data


if __name__ == "__main__":
    print("Testing Supabase connection...")

    tables = [
        "users",
        "markets",
        "trades",
        "reputation_snapshots",
    ]

    for table in tables:
        try:
            response = (
                supabase.table(table)
                .select("*")
                .limit(1)
                .execute()
            )
            print(f"SUCCESS: {table} accessible")
            print(f"  Rows returned: {len(response.data)}")

        except Exception as error:
            print(f"ERROR accessing {table}: {error}")


def save_reputation_snapshot(user_id, result, as_of):
    """Save a calculated reputation score to Supabase."""

    record = {
        "user_id": int(user_id),
        "as_of": as_of.isoformat(),
        "method_version": "prototype_v1",
        "n_positions": result["n_closed_lots"],
        "score": result["score"],
        "win_rate": result["win_rate"],
        "realized_edge": result["avg_edge_ticks"],
        "performance_score": result["performance_score"],
        "consistency_score": result["consistency_score"],
        "confidence_score": result["confidence_score"],
    }

    response = (
        supabase.table("reputation_snapshots")
        .upsert(
            record,
            on_conflict="user_id,as_of,method_version"
        )
        .execute()
    )

    return response.data

