
from dataclasses import dataclass
from collections import defaultdict
from datetime import datetime


@dataclass(frozen=True)
class Trade:
    user_id: str
    market_id: str
    side: str
    action: str
    price: int
    quantity: float
    executed_at: datetime | None = None


def reconstruct_positions(trades):
    """Reconstruct positions using average-cost matching.

    Trades must be provided in execution order.
    """
    positions = defaultdict(lambda: {'quantity': 0.0, 'cost': 0.0})
    closed = []

    for trade in trades:
        if trade.side not in ('YES', 'NO') or trade.action not in ('BUY', 'SELL'):
            raise ValueError('Invalid side or action')

        if not (1 <= trade.price <= 99) or trade.quantity <= 0:
            raise ValueError('Invalid price or quantity')

        key = (trade.user_id, trade.market_id, trade.side)
        pos = positions[key]

        if trade.action == 'BUY':
            pos['quantity'] += trade.quantity
            pos['cost'] += trade.price * trade.quantity

        else:
            if trade.quantity > pos['quantity'] + 1e-9:
                raise ValueError(f'Sell exceeds held position: {key}')

            avg_entry = pos['cost'] / pos['quantity']

            closed.append({
                'user_id': trade.user_id,
                'market_id': trade.market_id,
                'side': trade.side,
                'quantity': trade.quantity,
                'entry_price': avg_entry,
                'exit_price': trade.price,
                'closed_at': trade.executed_at,
                'close_reason': 'SELL'
            })

            pos['quantity'] -= trade.quantity
            pos['cost'] -= avg_entry * trade.quantity

            if pos['quantity'] < 1e-9:
                pos['quantity'] = 0.0
                pos['cost'] = 0.0

    opened = [
        {
            'user_id': u,
            'market_id': m,
            'side': s,
            'quantity': p['quantity'],
            'avg_entry_price': p['cost'] / p['quantity']
        }
        for (u, m, s), p in positions.items()
        if p['quantity'] > 1e-9
    ]

    return closed, opened


def settle_positions(open_positions, market_outcomes, resolved_at):
    """Settle winning contracts at 100 and losing contracts at 0.

    Assumes binary contracts use 0/100 settlement.
    """
    if resolved_at.tzinfo is None or resolved_at.utcoffset() is None:
        raise ValueError('resolved_at must be timezone-aware')

    settled = []
    remaining = []

    for pos in open_positions:
        outcome = market_outcomes.get(pos['market_id'])

        if outcome is None:
            remaining.append(pos)
            continue

        if outcome not in ('YES', 'NO'):
            raise ValueError('Invalid market outcome')

        settled.append({
            'user_id': pos['user_id'],
            'market_id': pos['market_id'],
            'side': pos['side'],
            'quantity': pos['quantity'],
            'entry_price': pos['avg_entry_price'],
            'exit_price': 100 if pos['side'] == outcome else 0,
            'closed_at': resolved_at,
            'close_reason': 'SETTLEMENT'
        })

    return settled, remaining
