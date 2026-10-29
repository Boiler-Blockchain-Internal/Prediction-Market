//! The sequence-number contract: `seq` must strictly increase, and a command that breaks the
//! rule is rejected with `StaleSeq` and changes nothing.
//!
//! Before this rule existed, replaying a `PlaceOrder` with an already-used `seq` hit the
//! "duplicate live order id" assert in the book and crashed the engine.

mod common;

use boilermarket_engine::{
    Action, Command, Engine, Event, EventKind, MarketId, OrderId, Outcome, RejectReason, Seq,
    TimeInForce, UserId,
};
use common::{place, registered_engine, MARKET};
use proptest::prelude::*;

#[path = "support/ops.rs"]
mod ops;
#[path = "support/reference.rs"]
mod reference;

/// True for exactly one `StaleSeq` rejection carrying the "no event stream" number zero.
fn is_stale(events: &[Event]) -> bool {
    events.len() == 1
        && events[0].event_no.get() == 0
        && matches!(
            events[0].kind,
            EventKind::OrderRejected {
                reason: RejectReason::StaleSeq,
                ..
            }
        )
}

#[test]
fn replayed_place_is_rejected_without_touching_the_book_or_event_numbers() {
    let mut engine = registered_engine(); // seq 1
    let first = place(
        &mut engine,
        2,
        9,
        Outcome::Yes,
        Action::Buy,
        50,
        10,
        TimeInForce::Gtc,
    );
    let last_event_no = first.last().unwrap().event_no.get();
    let before = engine.book_snapshot(MARKET);

    // Same seq again: used to panic with "duplicate live order id".
    let replay = place(
        &mut engine,
        2,
        9,
        Outcome::Yes,
        Action::Buy,
        50,
        10,
        TimeInForce::Gtc,
    );

    assert!(is_stale(&replay), "got {replay:?}");
    assert_eq!(replay[0].market, MARKET);
    assert_eq!(replay[0].seq, Seq::new(2));
    // The order id would be the stale seq, which belongs to a real live order, so it is omitted.
    assert!(matches!(
        &replay[0].kind,
        EventKind::OrderRejected { user: Some(u), order: None, .. } if *u == UserId::new(9)
    ));
    assert_eq!(engine.book_snapshot(MARKET), before);
    engine.check_invariants();

    // The next valid command continues the numbering with no gap.
    let next = place(
        &mut engine,
        3,
        8,
        Outcome::Yes,
        Action::Sell,
        70,
        1,
        TimeInForce::Gtc,
    );
    assert_eq!(next[0].event_no.get(), last_event_no + 1);
}

#[test]
fn lower_seq_is_rejected_and_a_rejection_does_not_move_the_high_water_mark() {
    let mut engine = registered_engine(); // seq 1
    place(
        &mut engine,
        10,
        1,
        Outcome::Yes,
        Action::Buy,
        50,
        5,
        TimeInForce::Gtc,
    );
    let before = engine.book_snapshot(MARKET);

    let lower = place(
        &mut engine,
        5,
        2,
        Outcome::Yes,
        Action::Sell,
        50,
        5,
        TimeInForce::Gtc,
    );
    assert!(is_stale(&lower), "got {lower:?}");
    assert_eq!(engine.book_snapshot(MARKET), before);

    // Seq 10 is still the high-water mark: 10 is stale, 11 is fine.
    let equal = place(
        &mut engine,
        10,
        2,
        Outcome::Yes,
        Action::Sell,
        50,
        5,
        TimeInForce::Gtc,
    );
    assert!(is_stale(&equal), "got {equal:?}");
    let next = place(
        &mut engine,
        11,
        2,
        Outcome::Yes,
        Action::Sell,
        50,
        5,
        TimeInForce::Gtc,
    );
    assert!(
        next.iter()
            .any(|e| matches!(e.kind, EventKind::TradeExecuted { .. })),
        "seq 11 should have matched the resting bid: {next:?}"
    );
}

#[test]
fn stale_cancel_reports_the_order_and_leaves_it_resting() {
    let mut engine = registered_engine();
    place(
        &mut engine,
        2,
        9,
        Outcome::Yes,
        Action::Buy,
        50,
        10,
        TimeInForce::Gtc,
    );
    let before = engine.book_snapshot(MARKET);

    // Seq 2 was already used by the place above, so this cancel is stale.
    let stale = engine.apply(Seq::new(2), cancel_order_2());
    assert!(is_stale(&stale), "got {stale:?}");
    assert!(matches!(
        &stale[0].kind,
        EventKind::OrderRejected { order: Some(o), .. } if *o == OrderId::new(2)
    ));
    assert_eq!(engine.book_snapshot(MARKET), before);

    // The same cancel under a fresh seq goes through.
    let done = engine.apply(Seq::new(3), cancel_order_2());
    assert!(matches!(done[0].kind, EventKind::OrderCancelled { .. }));
}

fn cancel_order_2() -> Command {
    Command::CancelOrder {
        user: UserId::new(9),
        market: MARKET,
        order: OrderId::new(2),
    }
}

#[test]
fn staleness_is_checked_before_the_market_is_looked_up() {
    let mut engine = Engine::new();
    engine.apply(
        Seq::new(5),
        Command::RegisterMarket {
            market: MarketId::new(1),
        },
    );
    let ghost = MarketId::new(99);

    let stale = engine.apply(Seq::new(5), Command::LockMarket { market: ghost });
    assert!(is_stale(&stale), "got {stale:?}");
    assert_eq!(stale[0].market, ghost);

    let unknown = engine.apply(Seq::new(6), Command::LockMarket { market: ghost });
    assert!(matches!(
        unknown[0].kind,
        EventKind::OrderRejected {
            reason: RejectReason::UnknownMarket,
            ..
        }
    ));
}

#[test]
fn first_command_may_use_any_seq_and_gaps_are_allowed() {
    let mut engine = Engine::new();
    let registered = engine.apply(Seq::new(0), Command::RegisterMarket { market: MARKET });
    assert!(matches!(registered[0].kind, EventKind::MarketRegistered));

    let placed = place(
        &mut engine,
        1_000,
        1,
        Outcome::Yes,
        Action::Buy,
        50,
        1,
        TimeInForce::Gtc,
    );
    assert!(matches!(placed[0].kind, EventKind::OrderAccepted { .. }));
}

#[test]
fn rejected_and_unknown_market_commands_still_consume_their_seq() {
    let mut engine = registered_engine();

    // A rejected order (bad price) uses up seq 2...
    let bad = place(
        &mut engine,
        2,
        1,
        Outcome::Yes,
        Action::Buy,
        0,
        5,
        TimeInForce::Gtc,
    );
    assert!(matches!(
        bad[0].kind,
        EventKind::OrderRejected {
            reason: RejectReason::BadPrice,
            ..
        }
    ));
    let again = place(
        &mut engine,
        2,
        1,
        Outcome::Yes,
        Action::Buy,
        50,
        5,
        TimeInForce::Gtc,
    );
    assert!(is_stale(&again), "got {again:?}");

    // ...and so does a command for an unknown market.
    let ghost = engine.apply(
        Seq::new(3),
        Command::LockMarket {
            market: MarketId::new(99),
        },
    );
    assert!(matches!(
        ghost[0].kind,
        EventKind::OrderRejected {
            reason: RejectReason::UnknownMarket,
            ..
        }
    ));
    let again = engine.apply(
        Seq::new(3),
        Command::LockMarket {
            market: MarketId::new(99),
        },
    );
    assert!(is_stale(&again), "got {again:?}");
}

fn xorshift(state: &mut u64) -> u64 {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    *state
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    /// Mixed random commands, with old commands replayed under their original `seq` at random
    /// points. Every replay must be a lone `StaleSeq` rejection, change no book, and leave the
    /// engine behaving exactly like the reference engine afterwards.
    #[test]
    fn replaying_old_commands_is_always_a_clean_no_op(
        stream in ops::stream(),
        seed in any::<u64>(),
    ) {
        let commands = ops::commands(&stream);
        let mut engine = Engine::new();
        let mut oracle = reference::Reference::new();
        let mut rng = seed | 1;
        let snapshots = |engine: &Engine| {
            (1..=4)
                .map(|m| engine.book_snapshot(MarketId::new(m)))
                .collect::<Vec<_>>()
        };

        for (index, command) in commands.iter().cloned().enumerate() {
            let seq = Seq::new(index as u64 + 1);
            let got = engine.apply(seq, command.clone());
            let want = oracle.apply(seq, command);
            prop_assert_eq!(&got, &want);
            for m in 1..=4 {
                let market = MarketId::new(m);
                prop_assert_eq!(engine.book_snapshot(market), oracle.book_snapshot(market));
            }

            if xorshift(&mut rng).is_multiple_of(3) {
                let j = (xorshift(&mut rng) % (index as u64 + 1)) as usize;
                let old_seq = Seq::new(j as u64 + 1);
                let before = snapshots(&engine);

                let replay = engine.apply(old_seq, commands[j].clone());
                prop_assert!(is_stale(&replay), "replay of command {} gave {:?}", j, replay);
                prop_assert_eq!(replay[0].seq, old_seq);
                prop_assert_eq!(&replay, &oracle.apply(old_seq, commands[j].clone()));
                prop_assert_eq!(before, snapshots(&engine));
                engine.check_invariants();
            }
        }
    }
}
