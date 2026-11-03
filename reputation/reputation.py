
"""Provisional reputation formula; weights require team approval."""

from math import exp
from scoring import weighted_metrics


def calculate_reputation(scored_lots, as_of, half_life_days=30):
    metrics = weighted_metrics(
        scored_lots, as_of, half_life_days
    )

    # Performance: 60%
    # Neutral edge maps to 50/100.
    performance = max(
        0.0,
        min(100.0, 50 + metrics['avg_edge_ticks'] / 2)
    )

    # Consistency: 25%
    consistency = 100 * metrics['win_rate']

    # Confidence: 15%
    # Temporary quantity-based evidence adjustment.
    confidence = 100 * (
        1 - exp(-metrics['effective_quantity'] / 10)
    )

    final_score = (
        0.60 * performance
        + 0.25 * consistency
        + 0.15 * confidence
    )

    return {
        'score': round(final_score, 2),
        'performance_score': round(performance, 2),
        'consistency_score': round(consistency, 2),
        'confidence_score': round(confidence, 2),
        'avg_edge_ticks': metrics['avg_edge_ticks'],
        'win_rate': metrics['win_rate'],
        'effective_quantity': metrics['effective_quantity'],
        'n_closed_lots': len(scored_lots)
    }
