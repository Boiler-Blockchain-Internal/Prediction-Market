mod common;

use boilermarket_engine::{
    Action, CancelReason, Command, EventKind, MarketId, OrderId, Outcome, Price, Qty, RejectReason,
    Seq, Side, TimeInForce, UserId, OPEN_ORDER_CAP,
};
use common::{place, registered_engine, MARKET};

fn kinds(events: &[boilermarket_engine::Event]) -> Vec<EventKind> {
    events.iter().map(|event| event.kind.clone()).collect()
}

#[test]
fn worked_example_a_matches_exactly() {
    let mut engine = registered_engine();
    place(
        &mut engine,
        2,
        9,
        Outcome::Yes,
        Action::Buy,
        58,
        10,
        TimeInForce::Gtc,
    );
    place(
        &mut engine,
        3,
        9,
        Outcome::Yes,
        Action::Buy,
        55,
        20,
        TimeInForce::Gtc,
    );
    place(
        &mut engine,
        4,
        9,
        Outcome::Yes,
        Action::Sell,
        61,
        15,
        TimeInForce::Gtc,
    );
    place(
        &mut engine,
        5,
        9,
        Outcome::Yes,
        Action::Sell,
        65,
        30,
        TimeInForce::Gtc,
    );

    let bob = place(
        &mut engine,
        6,
        2,
        Outcome::No,
        Action::Buy,
        40,
        25,
        TimeInForce::Gtc,
    );
    assert!(matches!(
        bob.last().map(|event| &event.kind),
        Some(EventKind::OrderRested {
            side: Side::Ask,
            price,
            remaining,
            ..
        }) if price.get() == 60 && remaining.get() == 25
    ));

    let carol = place(
        &mut engine,
        7,
        3,
        Outcome::Yes,
        Action::Buy,
        62,
        30,
        TimeInForce::Gtc,
    );
    assert_eq!(
        kinds(&carol),
        vec![
            EventKind::OrderAccepted {
                order: OrderId::new(7),
                user: UserId::new(3),
                outcome: Outcome::Yes,
                action: Action::Buy,
                typed_price: Price::new(62).unwrap(),
                side: Side::Bid,
                price: Price::new(62).unwrap(),
                qty: Qty::new(30),
                tif: TimeInForce::Gtc,
            },
            EventKind::TradeExecuted {
                maker: OrderId::new(6),
                taker: OrderId::new(7),
                maker_user: UserId::new(2),
                taker_user: UserId::new(3),
                price: Price::new(60).unwrap(),
                qty: Qty::new(25),
                taker_side: Side::Bid,
            },
            EventKind::OrderFilled {
                order: OrderId::new(6),
                user: UserId::new(2),
            },
            EventKind::TradeExecuted {
                maker: OrderId::new(4),
                taker: OrderId::new(7),
                maker_user: UserId::new(9),
                taker_user: UserId::new(3),
                price: Price::new(61).unwrap(),
                qty: Qty::new(5),
                taker_side: Side::Bid,
            },
            EventKind::OrderFilled {
                order: OrderId::new(7),
                user: UserId::new(3),
            },
        ]
    );
    let snapshot = engine.book_snapshot(MARKET).unwrap();
    let ask_61 = snapshot
        .asks
        .iter()
        .find(|level| level.price.get() == 61)
        .unwrap();
    assert_eq!(ask_61.total, Qty::new(10));
}

#[test]
fn worked_example_b_matches_exactly() {
    let mut engine = registered_engine();
    place(
        &mut engine,
        2,
        1,
        Outcome::Yes,
        Action::Sell,
        55,
        5,
        TimeInForce::Gtc,
    );
    place(
        &mut engine,
        3,
        2,
        Outcome::No,
        Action::Buy,
        42,
        10,
        TimeInForce::Gtc,
    );
    let carol = place(
        &mut engine,
        4,
        3,
        Outcome::Yes,
        Action::Buy,
        60,
        12,
        TimeInForce::Ioc,
    );

    let trades: Vec<_> = carol
        .iter()
        .filter_map(|event| match event.kind {
            EventKind::TradeExecuted { price, qty, .. } => Some((price.get(), qty.get())),
            _ => None,
        })
        .collect();
    assert_eq!(trades, vec![(55, 5), (58, 7)]);
    assert!(matches!(
        carol.last().map(|event| &event.kind),
        Some(EventKind::OrderFilled { order, .. }) if *order == OrderId::new(4)
    ));
    let snapshot = engine.book_snapshot(MARKET).unwrap();
    assert_eq!(snapshot.asks[0].price.get(), 58);
    assert_eq!(snapshot.asks[0].total.get(), 3);
}

#[test]
fn price_time_priority_partial_fill_and_multilevel_sweep() {
    let mut engine = registered_engine();
    place(
        &mut engine,
        2,
        1,
        Outcome::Yes,
        Action::Sell,
        50,
        3,
        TimeInForce::Gtc,
    );
    place(
        &mut engine,
        3,
        2,
        Outcome::Yes,
        Action::Sell,
        50,
        4,
        TimeInForce::Gtc,
    );
    place(
        &mut engine,
        4,
        3,
        Outcome::Yes,
        Action::Sell,
        51,
        5,
        TimeInForce::Gtc,
    );
    let events = place(
        &mut engine,
        5,
        4,
        Outcome::Yes,
        Action::Buy,
        51,
        10,
        TimeInForce::Gtc,
    );
    let makers: Vec<_> = events
        .iter()
        .filter_map(|event| match event.kind {
            EventKind::TradeExecuted { maker, qty, .. } => Some((maker.get(), qty.get())),
            _ => None,
        })
        .collect();
    assert_eq!(makers, vec![(2, 3), (3, 4), (4, 3)]);
    assert_eq!(engine.best_ask(MARKET).unwrap().get(), 51);
    assert_eq!(engine.book_snapshot(MARKET).unwrap().asks[0].total.get(), 2);
}

#[test]
fn ioc_stops_at_worst_price_and_expires_remainder() {
    let mut engine = registered_engine();
    place(
        &mut engine,
        2,
        1,
        Outcome::Yes,
        Action::Sell,
        60,
        2,
        TimeInForce::Gtc,
    );
    place(
        &mut engine,
        3,
        2,
        Outcome::Yes,
        Action::Sell,
        66,
        5,
        TimeInForce::Gtc,
    );
    let events = place(
        &mut engine,
        4,
        3,
        Outcome::Yes,
        Action::Buy,
        65,
        6,
        TimeInForce::Ioc,
    );
    assert!(events.iter().any(|event| matches!(
        event.kind,
        EventKind::TradeExecuted { price, qty, .. }
            if price.get() == 60 && qty.get() == 2
    )));
    assert!(matches!(
        events.last().map(|event| &event.kind),
        Some(EventKind::OrderExpired { unfilled, .. }) if unfilled.get() == 4
    ));
    assert_eq!(engine.best_ask(MARKET).unwrap().get(), 66);
}

#[test]
fn self_trade_cancels_maker_then_continues() {
    let mut engine = registered_engine();
    place(
        &mut engine,
        2,
        1,
        Outcome::Yes,
        Action::Sell,
        50,
        2,
        TimeInForce::Gtc,
    );
    place(
        &mut engine,
        3,
        2,
        Outcome::Yes,
        Action::Sell,
        51,
        3,
        TimeInForce::Gtc,
    );
    let events = place(
        &mut engine,
        4,
        1,
        Outcome::Yes,
        Action::Buy,
        51,
        3,
        TimeInForce::Gtc,
    );
    assert!(matches!(
        &events[1].kind,
        EventKind::OrderCancelled { order, reason: CancelReason::SelfTrade, .. }
            if *order == OrderId::new(2)
    ));
    assert!(matches!(
        &events[2].kind,
        EventKind::TradeExecuted { maker, .. } if *maker == OrderId::new(3)
    ));
}

#[test]
fn cancel_owner_checks_and_unknown_order() {
    let mut engine = registered_engine();
    place(
        &mut engine,
        2,
        1,
        Outcome::Yes,
        Action::Buy,
        40,
        2,
        TimeInForce::Gtc,
    );
    let wrong_owner = engine.apply(
        Seq::new(3),
        Command::CancelOrder {
            user: UserId::new(2),
            market: MARKET,
            order: OrderId::new(2),
        },
    );
    assert!(matches!(
        wrong_owner[0].kind,
        EventKind::OrderRejected {
            reason: RejectReason::NotOwner,
            ..
        }
    ));
    let unknown = engine.apply(
        Seq::new(4),
        Command::CancelOrder {
            user: UserId::new(1),
            market: MARKET,
            order: OrderId::new(999),
        },
    );
    assert!(matches!(
        unknown[0].kind,
        EventKind::OrderRejected {
            reason: RejectReason::OrderNotFound,
            ..
        }
    ));
    let cancelled = engine.apply(
        Seq::new(5),
        Command::CancelOrder {
            user: UserId::new(1),
            market: MARKET,
            order: OrderId::new(2),
        },
    );
    assert!(matches!(
        cancelled[0].kind,
        EventKind::OrderCancelled {
            reason: CancelReason::User,
            ..
        }
    ));
}

#[test]
fn lock_order_is_deterministic_and_orders_after_lock_are_rejected() {
    let mut engine = registered_engine();
    place(
        &mut engine,
        2,
        1,
        Outcome::Yes,
        Action::Buy,
        40,
        1,
        TimeInForce::Gtc,
    );
    place(
        &mut engine,
        3,
        2,
        Outcome::Yes,
        Action::Buy,
        60,
        1,
        TimeInForce::Gtc,
    );
    place(
        &mut engine,
        4,
        3,
        Outcome::Yes,
        Action::Sell,
        80,
        1,
        TimeInForce::Gtc,
    );
    place(
        &mut engine,
        5,
        4,
        Outcome::Yes,
        Action::Sell,
        70,
        1,
        TimeInForce::Gtc,
    );
    let locked = engine.apply(Seq::new(6), Command::LockMarket { market: MARKET });
    let cancelled: Vec<_> = locked
        .iter()
        .filter_map(|event| match event.kind {
            EventKind::OrderCancelled { order, .. } => Some(order.get()),
            _ => None,
        })
        .collect();
    assert_eq!(cancelled, vec![3, 2, 5, 4]);
    assert!(matches!(
        locked.last().unwrap().kind,
        EventKind::MarketLocked
    ));
    let rejected = place(
        &mut engine,
        7,
        5,
        Outcome::Yes,
        Action::Buy,
        50,
        1,
        TimeInForce::Gtc,
    );
    assert!(matches!(
        rejected[0].kind,
        EventKind::OrderRejected {
            reason: RejectReason::MarketNotOpen,
            ..
        }
    ));
}

#[test]
fn gate_checks_and_event_numbers_follow_contract() {
    let mut engine = boilermarket_engine::Engine::new();
    let unknown = place(
        &mut engine,
        1,
        1,
        Outcome::Yes,
        Action::Buy,
        50,
        1,
        TimeInForce::Gtc,
    );
    assert_eq!(unknown[0].event_no.get(), 0);
    assert!(matches!(
        unknown[0].kind,
        EventKind::OrderRejected {
            reason: RejectReason::UnknownMarket,
            ..
        }
    ));

    let registered = engine.apply(Seq::new(2), Command::RegisterMarket { market: MARKET });
    assert_eq!(registered[0].event_no.get(), 1);
    let duplicate = engine.apply(Seq::new(3), Command::RegisterMarket { market: MARKET });
    assert_eq!(duplicate[0].event_no.get(), 2);
    assert!(matches!(
        duplicate[0].kind,
        EventKind::OrderRejected {
            reason: RejectReason::MarketAlreadyExists,
            ..
        }
    ));
    let bad_price = place(
        &mut engine,
        4,
        1,
        Outcome::Yes,
        Action::Buy,
        0,
        1,
        TimeInForce::Gtc,
    );
    assert!(matches!(
        bad_price[0].kind,
        EventKind::OrderRejected {
            reason: RejectReason::BadPrice,
            ..
        }
    ));
    let bad_qty = place(
        &mut engine,
        5,
        1,
        Outcome::Yes,
        Action::Buy,
        50,
        0,
        TimeInForce::Gtc,
    );
    assert!(matches!(
        bad_qty[0].kind,
        EventKind::OrderRejected {
            reason: RejectReason::BadQuantity,
            ..
        }
    ));

    for offset in 0..OPEN_ORDER_CAP as u64 {
        place(
            &mut engine,
            10 + offset,
            9,
            Outcome::Yes,
            Action::Buy,
            10 + offset as u8,
            1,
            TimeInForce::Gtc,
        );
    }
    let capped = place(
        &mut engine,
        30,
        9,
        Outcome::Yes,
        Action::Buy,
        30,
        1,
        TimeInForce::Gtc,
    );
    assert!(matches!(
        capped[0].kind,
        EventKind::OrderRejected {
            reason: RejectReason::OpenOrderCap,
            ..
        }
    ));

    let all_events = [registered, duplicate, bad_price, bad_qty, capped];
    let mut last = 0;
    for batch in all_events {
        for event in batch {
            assert!(event.event_no.get() > last);
            last = event.event_no.get();
        }
    }
}

#[test]
fn no_sell_translates_to_yes_bid() {
    let mut engine = registered_engine();
    let events = place(
        &mut engine,
        2,
        1,
        Outcome::No,
        Action::Sell,
        35,
        2,
        TimeInForce::Gtc,
    );
    assert!(matches!(
        events.last().map(|event| &event.kind),
        Some(EventKind::OrderRested { side: Side::Bid, price, .. }) if price.get() == 65
    ));
}

#[test]
fn unknown_market_helpers_are_empty() {
    let engine = boilermarket_engine::Engine::new();
    assert_eq!(engine.best_bid(MarketId::new(999)), None);
    assert_eq!(engine.best_ask(MarketId::new(999)), None);
    assert_eq!(engine.book_snapshot(MarketId::new(999)), None);
}
