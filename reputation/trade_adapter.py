
from datetime import datetime
from positions import Trade
from database import fetch_trades


def load_trades():
    """Convert Supabase records into prototype Trade objects."""
    rows = fetch_trades()
    trades = []

    for row in rows:
        executed_at = datetime.fromisoformat(
            row["created_at"].replace("Z", "+00:00")
        )

        trade = Trade(
            user_id=str(row["user_id"]),
            market_id=str(row["market_id"]),
            side=row["side"].upper(),
            action=row["action"].upper(),
            price=round(float(row["price"]) * 100),
            quantity=float(row["shares"]),
            executed_at=executed_at,
        )

        trades.append(trade)

    trades.sort(key=lambda t: t.executed_at)
    return trades


if __name__ == "__main__":
    trades = load_trades()
    print(f"Successfully loaded {len(trades)} trades")
    for trade in trades:
        print(trade)
