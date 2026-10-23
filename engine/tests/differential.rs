//! Differential test: the real engine must emit exactly the same events, and end up with
//! exactly the same book, as the naive reference engine on every random command stream.

use boilermarket_engine::{Engine, MarketId, Seq};
use proptest::prelude::*;

#[path = "support/ops.rs"]
mod ops;
#[path = "support/reference.rs"]
mod reference;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2_000))]

    #[test]
    fn engine_matches_the_naive_reference(stream in ops::stream()) {
        let commands = ops::commands(&stream);
        let mut engine = Engine::new();
        let mut oracle = reference::Reference::new();

        for (index, command) in commands.into_iter().enumerate() {
            let seq = Seq::new(index as u64 + 1);
            let got = engine.apply(seq, command.clone());
            let want = oracle.apply(seq, command.clone());
            prop_assert_eq!(&got, &want, "events differ on command {} = {:?}", index, command);

            engine.check_invariants();
            for market in 1..=4 {
                let market = MarketId::new(market);
                prop_assert_eq!(
                    engine.book_snapshot(market),
                    oracle.book_snapshot(market),
                    "book differs for {:?} after command {} = {:?}",
                    market,
                    index,
                    command
                );
            }
        }
    }
}
