//! Shadow-book test: rebuild every book from the emitted events alone and require it to equal
//! the engine's own snapshot after every command.
//!
//! This is the contract the rest of the system will rely on. The Postgres projector, the
//! market-data feed and any replica only ever see events, never the engine's internal state.
//! If the events are missing something (a quantity, an order's price, a cancellation), a
//! consumer built from them silently drifts. This test finds that gap at the source.
//!
//! The shadow also checks each event against what it has rebuilt so far (a trade's maker must
//! be resting, at the traded price, on the opposite side; a rested order's remaining quantity
//! must equal its size minus its fills; locking must leave nothing resting; and so on), so a
//! wrong number inside an event is caught even when the books happen to agree.

use std::collections::BTreeMap;

use boilermarket_engine::{
    Action, BookSnapshot, Command, Engine, Event, EventKind, MarketId, MarketState, OrderId,
    Outcome, Price, Qty, Seq, Side, SnapshotLevel, SnapshotOrder, TimeInForce, UserId,
};
use proptest::prelude::*;

#[path = "support/ops.rs"]
mod ops;

macro_rules! ensure {
    ($cond:expr, $($msg:tt)+) => {
        if !$cond {
            return Err(format!($($msg)+));
        }
    };
}

/// An order that has been accepted in the current command but not yet closed out.
struct Accepted {
    user: UserId,
    outcome: Outcome,
    action: Action,
    typed_price: Price,
    side: Side,
    price: Price,
    qty: u64,
    tif: TimeInForce,
    traded: u64,
}

struct ShadowMarket {
    state: MarketState,
    /// Resting orders in arrival order, which is time priority within a price.
    resting: Vec<SnapshotOrder>,
}

#[derive(Default)]
struct Shadow {
    markets: BTreeMap<MarketId, ShadowMarket>,
    in_flight: BTreeMap<OrderId, Accepted>,
}

impl Shadow {
    fn apply(&mut self, event: &Event) -> Result<(), String> {
        match &event.kind {
            EventKind::MarketRegistered => {
                let old = self.markets.insert(
                    event.market,
                    ShadowMarket {
                        state: MarketState::Open,
                        resting: Vec::new(),
                    },
                );
                ensure!(old.is_none(), "{:?} registered twice", event.market);
            }
            EventKind::OrderRejected { .. } => {}
            EventKind::OrderAccepted {
                order,
                user,
                outcome,
                action,
                typed_price,
                side,
                price,
                qty,
                tif,
            } => {
                ensure!(
                    self.markets.contains_key(&event.market),
                    "order {order:?} accepted in an unknown market"
                );
                let old = self.in_flight.insert(
                    *order,
                    Accepted {
                        user: *user,
                        outcome: *outcome,
                        action: *action,
                        typed_price: *typed_price,
                        side: *side,
                        price: *price,
                        qty: qty.get(),
                        tif: *tif,
                        traded: 0,
                    },
                );
                ensure!(old.is_none(), "order {order:?} accepted twice");
            }
            EventKind::TradeExecuted {
                maker,
                taker,
                maker_user,
                taker_user,
                price,
                qty,
                taker_side,
            } => {
                ensure!(qty.get() > 0, "zero-size trade");
                let market = self
                    .markets
                    .get_mut(&event.market)
                    .ok_or("trade in an unknown market")?;
                let resting = market
                    .resting
                    .iter_mut()
                    .find(|o| o.order == *maker)
                    .ok_or_else(|| format!("trade maker {maker:?} is not resting"))?;
                ensure!(
                    resting.user == *maker_user,
                    "trade names {maker_user:?} as maker owner, book has {:?}",
                    resting.user
                );
                ensure!(
                    resting.price == *price,
                    "trade at {price:?} but maker rests at {:?}",
                    resting.price
                );
                ensure!(
                    resting.side != *taker_side,
                    "taker and maker are on the same side"
                );
                ensure!(
                    resting.remaining.get() >= qty.get(),
                    "trade of {} exceeds maker's remaining {}",
                    qty.get(),
                    resting.remaining.get()
                );
                resting.remaining = Qty::new(resting.remaining.get() - qty.get());

                let taker_info = self
                    .in_flight
                    .get_mut(taker)
                    .ok_or_else(|| format!("trade taker {taker:?} was never accepted"))?;
                ensure!(
                    taker_info.user == *taker_user,
                    "trade names {taker_user:?} as taker owner, accept said {:?}",
                    taker_info.user
                );
                ensure!(
                    taker_info.side == *taker_side,
                    "trade taker side disagrees with the accepted order"
                );
                taker_info.traded += qty.get();
                ensure!(
                    taker_info.traded <= taker_info.qty,
                    "taker {taker:?} traded more than its size"
                );
            }
            EventKind::OrderFilled { order, user } => {
                let market = self
                    .markets
                    .get_mut(&event.market)
                    .ok_or("fill in an unknown market")?;
                if let Some(pos) = market.resting.iter().position(|o| o.order == *order) {
                    // A resting order filled by the trade just before this event.
                    ensure!(market.resting[pos].user == *user, "fill names wrong owner");
                    ensure!(
                        market.resting[pos].remaining == Qty::ZERO,
                        "{order:?} reported filled with {:?} left",
                        market.resting[pos].remaining
                    );
                    market.resting.remove(pos);
                } else {
                    // An incoming order that filled completely and so never rested.
                    let done = self
                        .in_flight
                        .remove(order)
                        .ok_or_else(|| format!("fill for unknown order {order:?}"))?;
                    ensure!(done.user == *user, "fill names wrong owner");
                    ensure!(
                        done.traded == done.qty,
                        "{order:?} reported filled after trading {} of {}",
                        done.traded,
                        done.qty
                    );
                }
            }
            EventKind::OrderRested {
                order,
                user,
                side,
                price,
                remaining,
            } => {
                let accepted = self
                    .in_flight
                    .remove(order)
                    .ok_or_else(|| format!("rested order {order:?} was never accepted"))?;
                ensure!(accepted.user == *user, "rested order has wrong owner");
                ensure!(
                    accepted.side == *side && accepted.price == *price,
                    "rested order's side or price disagrees with the accepted order"
                );
                ensure!(accepted.tif == TimeInForce::Gtc, "an IOC order rested");
                ensure!(
                    remaining.get() > 0 && remaining.get() == accepted.qty - accepted.traded,
                    "{order:?} rested with {} but size {} minus fills {} is {}",
                    remaining.get(),
                    accepted.qty,
                    accepted.traded,
                    accepted.qty - accepted.traded
                );
                let market = self
                    .markets
                    .get_mut(&event.market)
                    .ok_or("rest in an unknown market")?;
                market.resting.push(SnapshotOrder {
                    order: *order,
                    user: *user,
                    outcome: accepted.outcome,
                    action: accepted.action,
                    typed_price: accepted.typed_price,
                    side: accepted.side,
                    price: accepted.price,
                    original: Qty::new(accepted.qty),
                    remaining: *remaining,
                    tif: accepted.tif,
                });
            }
            EventKind::OrderCancelled {
                order,
                user,
                remaining,
                ..
            } => {
                let market = self
                    .markets
                    .get_mut(&event.market)
                    .ok_or("cancel in an unknown market")?;
                let pos = market
                    .resting
                    .iter()
                    .position(|o| o.order == *order)
                    .ok_or_else(|| format!("cancelled order {order:?} is not resting"))?;
                ensure!(
                    market.resting[pos].user == *user,
                    "cancel names wrong owner"
                );
                ensure!(
                    market.resting[pos].remaining == *remaining,
                    "cancel reports {:?} left, book has {:?}",
                    remaining,
                    market.resting[pos].remaining
                );
                market.resting.remove(pos);
            }
            EventKind::OrderExpired {
                order,
                user,
                unfilled,
            } => {
                let accepted = self
                    .in_flight
                    .remove(order)
                    .ok_or_else(|| format!("expired order {order:?} was never accepted"))?;
                ensure!(accepted.user == *user, "expiry names wrong owner");
                ensure!(accepted.tif == TimeInForce::Ioc, "a GTC order expired");
                ensure!(
                    unfilled.get() > 0 && unfilled.get() == accepted.qty - accepted.traded,
                    "{order:?} expired with {} unfilled but size {} minus fills {} is {}",
                    unfilled.get(),
                    accepted.qty,
                    accepted.traded,
                    accepted.qty - accepted.traded
                );
            }
            EventKind::MarketLocked => {
                let market = self
                    .markets
                    .get_mut(&event.market)
                    .ok_or("lock of an unknown market")?;
                ensure!(
                    market.resting.is_empty(),
                    "market locked with {} orders still resting",
                    market.resting.len()
                );
                market.state = MarketState::Locked;
            }
        }
        Ok(())
    }

    /// Every accepted order must be closed out (filled, rested or expired) by the end of the
    /// command that accepted it.
    fn finish_command(&self) -> Result<(), String> {
        ensure!(
            self.in_flight.is_empty(),
            "orders accepted but never closed out: {:?}",
            self.in_flight.keys().collect::<Vec<_>>()
        );
        Ok(())
    }

    fn snapshot(&self, id: MarketId) -> Option<BookSnapshot> {
        let market = self.markets.get(&id)?;
        let level = |side: Side, raw: u8| -> Option<SnapshotLevel> {
            let orders: Vec<SnapshotOrder> = market
                .resting
                .iter()
                .filter(|o| o.side == side && o.price.get() == raw)
                .cloned()
                .collect();
            if orders.is_empty() {
                return None;
            }
            let total: u64 = orders.iter().map(|o| o.remaining.get()).sum();
            Some(SnapshotLevel {
                side,
                price: Price::new(raw).expect("loop price is valid"),
                total: Qty::new(total),
                orders,
            })
        };
        Some(BookSnapshot {
            market: id,
            state: market.state,
            bids: (1..=99).rev().filter_map(|p| level(Side::Bid, p)).collect(),
            asks: (1..=99).filter_map(|p| level(Side::Ask, p)).collect(),
        })
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2_000))]

    #[test]
    fn book_rebuilt_from_events_matches_the_engine(stream in ops::stream()) {
        let commands = ops::commands(&stream);
        let mut engine = Engine::new();
        let mut shadow = Shadow::default();

        for (index, command) in commands.into_iter().enumerate() {
            let events = engine.apply(Seq::new(index as u64 + 1), command.clone());
            for event in &events {
                shadow.apply(event).map_err(|why| {
                    TestCaseError::fail(format!("command {index} = {command:?}: {why}"))
                })?;
            }
            shadow.finish_command().map_err(|why| {
                TestCaseError::fail(format!("command {index} = {command:?}: {why}"))
            })?;

            for m in 1..=4 {
                let market = MarketId::new(m);
                prop_assert_eq!(
                    shadow.snapshot(market),
                    engine.book_snapshot(market),
                    "book rebuilt from events differs for {:?} after command {} = {:?}",
                    market,
                    index,
                    command
                );
            }
        }
    }
}

fn place_cmd(
    market: MarketId,
    user: u64,
    outcome: Outcome,
    action: Action,
    price: u8,
    qty: u64,
    tif: TimeInForce,
) -> Command {
    Command::PlaceOrder {
        user: UserId::new(user),
        market,
        outcome,
        action,
        price,
        qty,
        tif,
    }
}

/// Meta-test: the shadow must have teeth. Take a scripted run that touches every event type,
/// delete each event in turn, and require the shadow to notice (an error from a check, or a
/// book that no longer matches the engine's). If some event could be dropped unnoticed, a
/// consumer could drop it too.
#[test]
fn shadow_book_notices_any_single_dropped_event() {
    let m = MarketId::new(1);
    let commands = vec![
        Command::RegisterMarket { market: m },
        // Order 2 rests as a bid at 50.
        place_cmd(m, 1, Outcome::Yes, Action::Buy, 50, 10, TimeInForce::Gtc),
        // Order 3 sells into it: trade of 4 at 50, taker fully filled.
        place_cmd(m, 2, Outcome::Yes, Action::Sell, 50, 4, TimeInForce::Gtc),
        // Order 4 rests as a bid at 40, then user 3 cancels it with seq 5.
        place_cmd(m, 3, Outcome::Yes, Action::Buy, 40, 5, TimeInForce::Gtc),
        Command::CancelOrder {
            user: UserId::new(3),
            market: m,
            order: OrderId::new(4),
        },
        // Order 6 is IOC and finds nothing to trade, so it expires.
        place_cmd(m, 4, Outcome::Yes, Action::Buy, 30, 3, TimeInForce::Ioc),
        // Locking cancels the 6 shares still resting from order 2.
        Command::LockMarket { market: m },
    ];

    let mut engine = Engine::new();
    let events: Vec<Event> = commands
        .into_iter()
        .enumerate()
        .flat_map(|(i, command)| engine.apply(Seq::new(i as u64 + 1), command))
        .collect();
    let engine_book = engine.book_snapshot(m);

    let rebuild = |events: &[&Event]| -> Result<Option<BookSnapshot>, String> {
        let mut shadow = Shadow::default();
        for event in events {
            shadow.apply(event)?;
        }
        shadow.finish_command()?;
        Ok(shadow.snapshot(m))
    };

    let everything: Vec<&Event> = events.iter().collect();
    assert_eq!(
        rebuild(&everything),
        Ok(engine_book.clone()),
        "the full event stream must rebuild the engine's book"
    );

    for dropped in 0..events.len() {
        let without: Vec<&Event> = events
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != dropped)
            .map(|(_, event)| event)
            .collect();
        if let Ok(book) = rebuild(&without) {
            assert_ne!(
                book, engine_book,
                "dropping event {dropped} ({:?}) went unnoticed",
                events[dropped].kind
            );
        }
    }
}
