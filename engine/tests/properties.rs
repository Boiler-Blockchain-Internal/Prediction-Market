use std::collections::BTreeMap;

use boilermarket_engine::{
    Action, Command, Engine, EventKind, MarketId, OrderId, Outcome, Seq, TimeInForce, UserId,
};
use proptest::prelude::*;

#[derive(Clone, Debug)]
struct RawOrder {
    user: u64,
    outcome: bool,
    buy: bool,
    price: u8,
    qty: u64,
    gtc: bool,
}

fn raw_order() -> impl Strategy<Value = RawOrder> {
    (
        1_u64..=5,
        any::<bool>(),
        any::<bool>(),
        35_u8..=65,
        1_u64..=20,
        any::<bool>(),
    )
        .prop_map(|(user, outcome, buy, price, qty, gtc)| RawOrder {
            user,
            outcome,
            buy,
            price,
            qty,
            gtc,
        })
}

fn run(
    stream: &[RawOrder],
) -> (
    Vec<boilermarket_engine::Event>,
    boilermarket_engine::BookSnapshot,
) {
    let market = MarketId::new(1);
    let mut engine = Engine::new();
    let mut events = engine.apply(Seq::new(1), Command::RegisterMarket { market });
    for (index, raw) in stream.iter().enumerate() {
        events.extend(engine.apply(
            Seq::new(index as u64 + 2),
            Command::PlaceOrder {
                user: UserId::new(raw.user),
                market,
                outcome: if raw.outcome {
                    Outcome::Yes
                } else {
                    Outcome::No
                },
                action: if raw.buy { Action::Buy } else { Action::Sell },
                price: raw.price,
                qty: raw.qty,
                tif: if raw.gtc {
                    TimeInForce::Gtc
                } else {
                    TimeInForce::Ioc
                },
            },
        ));
    }
    (events, engine.book_snapshot(market).unwrap())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1_000))]

    #[test]
    fn random_streams_are_deterministic_gap_free_and_conservative(
        stream in prop::collection::vec(raw_order(), 0..100)
    ) {
        let first = run(&stream);
        let second = run(&stream);
        prop_assert_eq!(&first, &second);

        for (index, event) in first.0.iter().enumerate() {
            prop_assert_eq!(event.event_no.get(), index as u64 + 1);
        }

        let mut original = BTreeMap::<OrderId, u64>::new();
        let mut traded = BTreeMap::<OrderId, u64>::new();
        for event in &first.0 {
            match event.kind {
                EventKind::OrderAccepted { order, qty, .. } => {
                    original.insert(order, qty.get());
                }
                EventKind::TradeExecuted { maker, taker, qty, .. } => {
                    *traded.entry(maker).or_insert(0) += qty.get();
                    *traded.entry(taker).or_insert(0) += qty.get();
                }
                _ => {}
            }
        }
        for (order, total) in traded {
            prop_assert!(total <= original[&order]);
        }
    }
}
