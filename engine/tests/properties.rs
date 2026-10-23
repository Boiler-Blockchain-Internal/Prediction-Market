use std::collections::BTreeMap;

use boilermarket_engine::{
    Action, Command, Engine, Event, EventKind, MarketId, OrderId, Outcome, RejectReason, Seq,
    TimeInForce, UserId,
};
use proptest::prelude::*;

#[path = "support/ops.rs"]
mod ops;

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

fn replay(commands: &[Command]) -> Vec<Event> {
    let mut engine = Engine::new();
    commands
        .iter()
        .cloned()
        .enumerate()
        .flat_map(|(i, command)| engine.apply(Seq::new(i as u64 + 1), command))
        .collect()
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1_000))]

    /// Full command mix (place, cancel, lock, register, bad input, unknown markets) with the
    /// structural invariants checked after every single command, plus the event contract:
    /// per-market event numbers are consecutive, every accepted order ends the command in
    /// exactly one closing event, and quantity is conserved.
    #[test]
    fn mixed_command_streams_hold_invariants_and_the_event_contract(
        stream in ops::stream()
    ) {
        let commands = ops::commands(&stream);
        let mut engine = Engine::new();
        let mut all_events = Vec::new();
        let mut next_event_no = BTreeMap::<MarketId, u64>::new();
        let mut original = BTreeMap::<OrderId, u64>::new();
        let mut traded = BTreeMap::<OrderId, u64>::new();

        for (index, command) in commands.iter().cloned().enumerate() {
            let seq = Seq::new(index as u64 + 1);
            let events = engine.apply(seq, command);
            engine.check_invariants();

            prop_assert!(!events.is_empty(), "every command produces at least one event");

            for event in &events {
                prop_assert_eq!(event.seq, seq);
                if event.event_no.get() == 0 {
                    // Event number zero is reserved for rejections about unknown markets.
                    let is_unknown_market_reject = matches!(
                        event.kind,
                        EventKind::OrderRejected { reason: RejectReason::UnknownMarket, .. }
                    );
                    prop_assert!(is_unknown_market_reject, "event_no 0 on {:?}", event.kind);
                    prop_assert!(engine.book_snapshot(event.market).is_none());
                } else {
                    let expected = next_event_no.entry(event.market).or_insert(1);
                    prop_assert_eq!(event.event_no.get(), *expected);
                    *expected += 1;
                }

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

            // An accepted order is always closed out within the same command: it fills,
            // rests, or expires, and that is the final event emitted.
            if let EventKind::OrderAccepted { order, .. } = &events[0].kind {
                let accepted = *order;
                let closes_it = match &events[events.len() - 1].kind {
                    EventKind::OrderFilled { order, .. }
                    | EventKind::OrderRested { order, .. }
                    | EventKind::OrderExpired { order, .. } => *order == accepted,
                    _ => false,
                };
                prop_assert!(closes_it, "accepted order {:?} not closed by last event", accepted);
            }

            all_events.extend(events);
        }

        for (order, total) in traded {
            prop_assert!(total <= original[&order], "order {:?} over-traded", order);
        }

        // Determinism: a fresh engine fed the same commands emits identical events.
        prop_assert_eq!(all_events, replay(&commands));
    }
}
