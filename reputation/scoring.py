
from math import isfinite


def realized_edge(entry_price, exit_price):
    """Realized price difference in ticks per contract."""
    return exit_price - entry_price


def score_closed_lots(closed_lots):
    return [
        {
            **lot,
            'realized_edge_ticks': realized_edge(
                lot['entry_price'], lot['exit_price']
            ),
            'total_edge_ticks': realized_edge(
                lot['entry_price'], lot['exit_price']
            ) * lot['quantity']
        }
        for lot in closed_lots
    ]


def brier_score(probability, outcome):
    """Measure probability forecast accuracy."""
    if not 0 <= probability <= 1 or outcome not in (0, 1):
        raise ValueError('Invalid probability or outcome')

    return (probability - outcome) ** 2


def directional_correct(probability, outcome):
    """Measure whether the probability favored the correct outcome."""
    if not 0 <= probability <= 1 or outcome not in (0, 1):
        raise ValueError('Invalid probability or outcome')

    if probability == 0.5:
        return None

    return int((probability > 0.5) == bool(outcome))


def decay_weight(closed_at, as_of, half_life_days=30):
    """Older trades receive less weight in reputation calculations."""
    if half_life_days <= 0:
        raise ValueError('Half-life must be positive')

    if closed_at is None or any(
        d.tzinfo is None or d.utcoffset() is None
        for d in (closed_at, as_of)
    ):
        raise ValueError('Timezone-aware timestamps required')

    age_days = (as_of - closed_at).total_seconds() / 86400

    if age_days < 0:
        raise ValueError('Closure cannot be in the future')

    return 2 ** (-age_days / half_life_days)


def weighted_metrics(scored_lots, as_of, half_life_days=30):
    """Calculate recency- and quantity-weighted performance."""
    total_weight = 0.0
    weighted_edge = 0.0
    weighted_wins = 0.0

    for lot in scored_lots:
        quantity = lot['quantity']
        edge = lot['realized_edge_ticks']

        if not isfinite(quantity) or quantity <= 0 or not isfinite(edge):
            raise ValueError('Invalid scored lot')

        weight = quantity * decay_weight(
            lot.get('closed_at'), as_of, half_life_days
        )

        total_weight += weight
        weighted_edge += weight * edge
        weighted_wins += weight * (edge > 0)

    if total_weight == 0:
        return {
            'avg_edge_ticks': 0.0,
            'win_rate': 0.0,
            'effective_quantity': 0.0
        }

    return {
        'avg_edge_ticks': weighted_edge / total_weight,
        'win_rate': weighted_wins / total_weight,
        'effective_quantity': total_weight
    }
