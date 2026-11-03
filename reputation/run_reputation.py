
from collections import defaultdict
from datetime import datetime, timezone

from database import save_reputation_snapshot
from trade_adapter import load_trades
from positions import reconstruct_positions
from scoring import score_closed_lots
from reputation import calculate_reputation


def main():
    # 1. Fetch trades from Supabase
    trades = load_trades()
    print(f"Loaded {len(trades)} trades")

    # 2. Reconstruct open and closed positions
    closed, opened = reconstruct_positions(trades)
    scored_lots = score_closed_lots(closed)

    print(f"Closed lots: {len(closed)}")
    print(f"Open positions: {len(opened)}")

    # 3. Group closed lots by user
    by_user = defaultdict(list)

    for lot in scored_lots:
        by_user[lot["user_id"]].append(lot)

    # 4. Calculate reputation for each user
    as_of = datetime.now(timezone.utc)

    for user_id, lots in by_user.items():
        result = calculate_reputation(lots, as_of)

        # 5. Save reputation to Supabase
        response = save_reputation_snapshot(
            user_id,
            result,
            as_of
        )

        # Debug: Check what Supabase returned
        print(f"\nDatabase response for User {user_id}: {response}")

        # 6. Display calculated results
        print(f"\nUser {user_id}")
        print(f"  Reputation score: {result['score']}")
        print(f"  Average realized edge: {result['avg_edge_ticks']:.2f} ticks")
        print(f"  Win rate: {result['win_rate']:.2%}")
        print(f"  Closed lots: {result['n_closed_lots']}")
        print(f"  Performance: {result['performance_score']}")
        print(f"  Consistency: {result['consistency_score']}")
        print(f"  Confidence: {result['confidence_score']}")

    # 7. Display open positions
    print("\nOPEN POSITIONS")

    for pos in opened:
        print(pos)


if __name__ == "__main__":
    main()
