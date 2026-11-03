use std::collections::BTreeMap;

use crate::market::OrderRequest;
use crate::{
    BookSnapshot, Command, Event, EventKind, EventNo, Market, MarketId, OrderId, Price,
    RejectReason, Seq,
};

#[derive(Clone, Debug, Default)]
pub struct Engine {
    markets: BTreeMap<MarketId, Market>,
}

impl Engine {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn apply(&mut self, seq: Seq, cmd: Command) -> Vec<Event> {
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
