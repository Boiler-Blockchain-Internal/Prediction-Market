
from pathlib import Path
import sys
from positions import Trade, reconstruct_positions
from scoring import score_closed_lots, brier_score, directional_correct

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))


trades = [
    Trade('alice', 'election', 'YES', 'BUY', 40, 5),
    Trade('alice', 'election', 'YES', 'BUY', 50, 5),
    Trade('alice', 'election', 'YES', 'SELL', 60, 6),
    Trade('bob', 'election', 'NO', 'BUY', 65, 4),
    Trade('bob', 'election', 'NO', 'SELL', 55, 4),
    Trade('cara', 'weather', 'YES', 'BUY', 30, 3),
]
closed, open_positions = reconstruct_positions(trades)
print('CLOSED LOTS / REALIZED SCORES')
for item in score_closed_lots(closed):
    print(item)
print('\nOPEN POSITIONS (NOT REALIZED)')
for item in open_positions:
    print(item)
print('\nRESOLVED EXAMPLE: election YES outcome = 1')
print('Alice entry forecast: Brier =', brier_score(.45, 1),
      '; directional correct =', directional_correct(.45, 1))
