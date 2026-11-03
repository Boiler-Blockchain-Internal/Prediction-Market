
import unittest
from datetime import datetime, timedelta, timezone

from positions import Trade, reconstruct_positions, settle_positions
from scoring import (
    realized_edge,
    score_closed_lots,
    brier_score,
    directional_correct,
    decay_weight,
    weighted_metrics
)
from reputation import calculate_reputation


NOW = datetime(2026, 10, 29, tzinfo=timezone.utc)


class TestReputation(unittest.TestCase):

    def test_weighted_entry_and_partial_exit(self):
        trades = [
            Trade('a', 'm', 'YES', 'BUY', 40, 5),
            Trade('a', 'm', 'YES', 'BUY', 50, 5),
            Trade('a', 'm', 'YES', 'SELL', 60, 6, NOW)
        ]

        closed, opened = reconstruct_positions(trades)

        self.assertEqual(closed[0]['entry_price'], 45)
        self.assertEqual(
            score_closed_lots(closed)[0]['realized_edge_ticks'], 15
        )
        self.assertEqual(opened[0]['quantity'], 4)
        self.assertEqual(closed[0]['closed_at'], NOW)

    def test_loss(self):
        self.assertEqual(realized_edge(65, 55), -10)

    def test_no_naked_sell(self):
        with self.assertRaises(ValueError):
            reconstruct_positions([
                Trade('a', 'm', 'NO', 'SELL', 30, 1)
            ])

    def test_brier(self):
        self.assertAlmostEqual(brier_score(0.8, 1), 0.04)
        self.assertEqual(directional_correct(0.8, 1), 1)
        self.assertIsNone(directional_correct(0.5, 1))

    def test_decay(self):
        self.assertAlmostEqual(
            decay_weight(NOW - timedelta(days=30), NOW), 0.5
        )
        self.assertAlmostEqual(
            decay_weight(NOW - timedelta(days=60), NOW), 0.25
        )

    def test_recency_changes_average(self):
        lots = score_closed_lots([
            {
                'entry_price': 40,
                'exit_price': 60,
                'quantity': 1,
                'closed_at': NOW
            },
            {
                'entry_price': 60,
                'exit_price': 40,
                'quantity': 1,
                'closed_at': NOW - timedelta(days=30)
            }
        ])

        self.assertAlmostEqual(
            weighted_metrics(lots, NOW)['avg_edge_ticks'],
            20 / 3
        )

    def test_settlement(self):
        _, opened = reconstruct_positions([
            Trade('a', 'm', 'NO', 'BUY', 35, 2)
        ])

        settled, remaining = settle_positions(
            opened, {'m': 'NO'}, NOW
        )

        self.assertEqual(settled[0]['exit_price'], 100)
        self.assertEqual(settled[0]['close_reason'], 'SETTLEMENT')
        self.assertFalse(remaining)

    def test_reputation_range(self):
        lots = score_closed_lots([
            {
                'entry_price': 40,
                'exit_price': 60,
                'quantity': 5,
                'closed_at': NOW
            }
        ])

        result = calculate_reputation(lots, NOW)

        self.assertGreaterEqual(result['score'], 0)
        self.assertLessEqual(result['score'], 100)

    def test_missing_timestamp_fails_decay(self):
        with self.assertRaises(ValueError):
            decay_weight(None, NOW)


if __name__ == '__main__':
    unittest.main()
