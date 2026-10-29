use std::collections::BTreeMap;

use crate::market::OrderRequest;
use crate::{
    BookSnapshot, Command, Event, EventKind, EventNo, Market, MarketId, OrderId, Price,
    RejectReason, Seq,
};

#[derive(Clone, Debug, Default)]
pub struct Engine {
    markets: BTreeMap<MarketId, Market>,
    /// Highest `seq` applied so far. Order ids are derived from `seq`, so requiring `seq` to
    /// strictly increase is what makes order ids unique and replays safe.
    last_seq: Option<Seq>,
}

impl Engine {
    pub fn new() -> Self {
        Self::default()
    }

    /// Applies one sequenced command and returns the events it produced.
    ///
    /// `seq` must be strictly greater than the `seq` of every command applied before. A
    /// command that breaks this is rejected with [`RejectReason::StaleSeq`] (one event,
    /// `event_no = 0`) and changes nothing, so a duplicated or replayed command cannot
    /// corrupt state or create a second order with the same id.
    pub fn apply(&mut self, seq: Seq, cmd: Command) -> Vec<Event> {
        if self.last_seq.is_some_and(|last| seq <= last) {
            return vec![stale_seq_event(seq, &cmd)];
        }
        self.last_seq = Some(seq);

        match cmd {
            Command::RegisterMarket { market } => {
                let kind = match self.markets.entry(market) {
                    std::collections::btree_map::Entry::Occupied(_) => EventKind::OrderRejected {
                        user: None,
                        reason: RejectReason::MarketAlreadyExists,
                        order: None,
                    },
                    std::collections::btree_map::Entry::Vacant(entry) => {
                        entry.insert(Market::new(market));
                        EventKind::MarketRegistered
                    }
                };
                self.markets
                    .get(&market)
                    .expect("registered market must exist")
                    .debug_check();
                self.emit(market, seq, vec![kind])
            }
            Command::PlaceOrder {
                user,
                market,
                outcome,
                action,
                price,
                qty,
                tif,
            } => {
                let Some(book) = self.markets.get_mut(&market) else {
                    return vec![unknown_market_event(
                        market,
                        seq,
                        Some(user),
                        Some(OrderId::new(seq.get())),
                    )];
                };
                let kinds = book.place_order(OrderRequest {
                    order: OrderId::new(seq.get()),
                    user,
                    outcome,
                    action,
                    raw_price: price,
                    raw_qty: qty,
                    tif,
                });
                book.debug_check();
                self.emit(market, seq, kinds)
            }
            Command::CancelOrder {
                user,
                market,
                order,
            } => {
                let Some(book) = self.markets.get_mut(&market) else {
                    return vec![unknown_market_event(market, seq, Some(user), Some(order))];
                };
                let kinds = book.cancel_order(user, order);
                book.debug_check();
                self.emit(market, seq, kinds)
            }
            Command::LockMarket { market } => {
                let Some(book) = self.markets.get_mut(&market) else {
                    return vec![unknown_market_event(market, seq, None, None)];
                };
                let kinds = book.lock();
                book.debug_check();
                self.emit(market, seq, kinds)
            }
        }
    }

    pub fn best_bid(&self, market: MarketId) -> Option<Price> {
        self.markets.get(&market).and_then(Market::best_bid)
    }

    pub fn best_ask(&self, market: MarketId) -> Option<Price> {
        self.markets.get(&market).and_then(Market::best_ask)
    }

    pub fn book_snapshot(&self, market: MarketId) -> Option<BookSnapshot> {
        self.markets.get(&market).map(Market::snapshot)
    }

    /// Asserts every structural invariant of every market (book never crossed, level totals
    /// match their orders, order index and queues agree, and so on). Panics on a violation.
    ///
    /// Unlike the internal per-command check, this is compiled in every build profile, so
    /// tests do not silently lose their safety net under `--release`.
    pub fn check_invariants(&self) {
        for market in self.markets.values() {
            market.check_invariants();
        }
    }

    fn emit(&mut self, market: MarketId, seq: Seq, kinds: Vec<EventKind>) -> Vec<Event> {
        let book = self
            .markets
            .get_mut(&market)
            .expect("events can only be emitted for a registered market");
        kinds
            .into_iter()
            .map(|kind| Event {
                market,
                seq,
                event_no: book.take_event_no(),
                kind,
            })
            .collect()
    }
}

fn stale_seq_event(seq: Seq, cmd: &Command) -> Event {
    // Like an unknown market, a stale command has no event stream of its own to advance, so
    // it uses the event number zero sentinel. `order` is left empty for a place command
    // because the order id would be the stale `seq`, which may belong to a real live order.
    let (user, order) = match *cmd {
        Command::PlaceOrder { user, .. } => (Some(user), None),
        Command::CancelOrder { user, order, .. } => (Some(user), Some(order)),
        Command::RegisterMarket { .. } | Command::LockMarket { .. } => (None, None),
    };
    Event {
        market: cmd.market(),
        seq,
        event_no: EventNo::new(0),
        kind: EventKind::OrderRejected {
            user,
            reason: RejectReason::StaleSeq,
            order,
        },
    }
}

fn unknown_market_event(
    market: MarketId,
    seq: Seq,
    user: Option<crate::UserId>,
    order: Option<OrderId>,
) -> Event {
    // Unknown markets have no event stream. Event number zero is the documented sentinel and
    // does not advance any registered market's counter.
    Event {
        market,
        seq,
        event_no: EventNo::new(0),
        kind: EventKind::OrderRejected {
            user,
            reason: RejectReason::UnknownMarket,
            order,
        },
    }
}
