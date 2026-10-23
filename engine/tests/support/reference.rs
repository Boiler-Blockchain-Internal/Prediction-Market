//! A deliberately naive matching engine used as an oracle for differential testing.
//!
//! Resting orders live in one `Vec` per market, in arrival order, and every question is
//! answered by a linear scan. There are no price-level arrays, no linked lists, no slab and
//! no hash maps. It is slow and easy to read, which is the point: if the real engine and
//! this one ever disagree, one of them is wrong, and this one is simple enough to audit.
//!
//! It follows the same contract as the real engine: the order of events inside a command,
//! the reject-reason precedence, and per-market event numbering (with event number 0 for
//! rejections about markets that do not exist).

use std::collections::btree_map::Entry;
use std::collections::BTreeMap;

use boilermarket_engine::{
    Action, BookSnapshot, CancelReason, Command, Event, EventKind, EventNo, MarketId, MarketState,
    OrderId, Outcome, Price, Qty, RejectReason, Seq, Side, SnapshotLevel, SnapshotOrder,
    TimeInForce, UserId, MAX_QTY, OPEN_ORDER_CAP,
};

#[derive(Clone)]
struct Resting {
    id: OrderId,
    user: UserId,
    outcome: Outcome,
    action: Action,
    typed_price: Price,
    side: Side,
    price: Price,
    original: u64,
    remaining: u64,
    tif: TimeInForce,
}

struct RefMarket {
    state: MarketState,
    /// Resting orders in arrival order. Removal keeps the relative order of the rest, so
    /// "earliest in this Vec" is always "oldest", which is exactly time priority.
    resting: Vec<Resting>,
    next_event_no: u64,
}

#[derive(Default)]
pub struct Reference {
    markets: BTreeMap<MarketId, RefMarket>,
}

fn reject(user: Option<UserId>, reason: RejectReason, order: Option<OrderId>) -> EventKind {
    EventKind::OrderRejected {
        user,
        reason,
        order,
    }
}

fn unknown_market(
    market: MarketId,
    seq: Seq,
    user: Option<UserId>,
    order: Option<OrderId>,
) -> Vec<Event> {
    vec![Event {
        market,
        seq,
        event_no: EventNo::new(0),
        kind: reject(user, RejectReason::UnknownMarket, order),
    }]
}

/// Index of the best resting order that an incoming order on `incoming` side with the given
/// limit would trade against: best price first, then earliest arrival.
fn best_maker(resting: &[Resting], incoming: Side, limit: u8) -> Option<usize> {
    let mut best: Option<usize> = None;
    for (i, order) in resting.iter().enumerate() {
        let crosses = match incoming {
            Side::Bid => order.side == Side::Ask && order.price.get() <= limit,
            Side::Ask => order.side == Side::Bid && order.price.get() >= limit,
        };
        if !crosses {
            continue;
        }
        let better = match best {
            None => true,
            // Strictly better price only; on a tie the earlier arrival (already `best`) wins.
            Some(b) => match incoming {
                Side::Bid => order.price < resting[b].price,
                Side::Ask => order.price > resting[b].price,
            },
        };
        if better {
            best = Some(i);
        }
    }
    best
}

impl RefMarket {
    #[allow(clippy::too_many_arguments)]
    fn place(
        &mut self,
        order: OrderId,
        user: UserId,
        outcome: Outcome,
        action: Action,
        raw_price: u8,
        raw_qty: u64,
        tif: TimeInForce,
    ) -> Vec<EventKind> {
        if self.state != MarketState::Open {
            return vec![reject(Some(user), RejectReason::MarketNotOpen, Some(order))];
        }
        let Ok(typed_price) = Price::new(raw_price) else {
            return vec![reject(Some(user), RejectReason::BadPrice, Some(order))];
        };
        if !(1..=MAX_QTY).contains(&raw_qty) {
            return vec![reject(Some(user), RejectReason::BadQuantity, Some(order))];
        }
        let open = self.resting.iter().filter(|o| o.user == user).count() as u32;
        if open >= OPEN_ORDER_CAP {
            return vec![reject(Some(user), RejectReason::OpenOrderCap, Some(order))];
        }

        // A YES price p and a NO price 100 - p are the same bet from opposite sides.
        let complement = Price::new(100 - raw_price).expect("complement of a valid price");
        let (side, price) = match (outcome, action) {
            (Outcome::Yes, Action::Buy) => (Side::Bid, typed_price),
            (Outcome::Yes, Action::Sell) => (Side::Ask, typed_price),
            (Outcome::No, Action::Buy) => (Side::Ask, complement),
            (Outcome::No, Action::Sell) => (Side::Bid, complement),
        };

        let mut remaining = raw_qty;
        let mut events = vec![EventKind::OrderAccepted {
            order,
            user,
            outcome,
            action,
            typed_price,
            side,
            price,
            qty: Qty::new(raw_qty),
            tif,
        }];

        while remaining > 0 {
            let Some(i) = best_maker(&self.resting, side, price.get()) else {
                break;
            };

            if self.resting[i].user == user {
                // Self-trade prevention: cancel the resting order, keep matching.
                let gone = self.resting.remove(i);
                events.push(EventKind::OrderCancelled {
                    order: gone.id,
                    user: gone.user,
                    reason: CancelReason::SelfTrade,
                    remaining: Qty::new(gone.remaining),
                });
                continue;
            }

            let fill = remaining.min(self.resting[i].remaining);
            remaining -= fill;
            self.resting[i].remaining -= fill;
            let maker = &self.resting[i];
            events.push(EventKind::TradeExecuted {
                maker: maker.id,
                taker: order,
                maker_user: maker.user,
                taker_user: user,
                price: maker.price,
                qty: Qty::new(fill),
                taker_side: side,
            });
            if self.resting[i].remaining == 0 {
                let gone = self.resting.remove(i);
                events.push(EventKind::OrderFilled {
                    order: gone.id,
                    user: gone.user,
                });
            }
        }

        if remaining == 0 {
            events.push(EventKind::OrderFilled { order, user });
        } else if tif == TimeInForce::Gtc {
            self.resting.push(Resting {
                id: order,
                user,
                outcome,
                action,
                typed_price,
                side,
                price,
                original: raw_qty,
                remaining,
                tif,
            });
            events.push(EventKind::OrderRested {
                order,
                user,
                side,
                price,
                remaining: Qty::new(remaining),
            });
        } else {
            events.push(EventKind::OrderExpired {
                order,
                user,
                unfilled: Qty::new(remaining),
            });
        }
        events
    }

    fn cancel(&mut self, user: UserId, order: OrderId) -> Vec<EventKind> {
        let Some(i) = self.resting.iter().position(|o| o.id == order) else {
            return vec![reject(Some(user), RejectReason::OrderNotFound, Some(order))];
        };
        if self.resting[i].user != user {
            return vec![reject(Some(user), RejectReason::NotOwner, Some(order))];
        }
        let gone = self.resting.remove(i);
        vec![EventKind::OrderCancelled {
            order: gone.id,
            user: gone.user,
            reason: CancelReason::User,
            remaining: Qty::new(gone.remaining),
        }]
    }

    fn lock(&mut self) -> Vec<EventKind> {
        if self.state != MarketState::Open {
            return vec![reject(None, RejectReason::MarketNotOpen, None)];
        }
        // Cancel order: bids from the highest price down, then asks from the lowest price
        // up, oldest first within a price. `sort_by` is stable, so arrival order survives.
        let mut bids: Vec<Resting> = self
            .resting
            .iter()
            .filter(|o| o.side == Side::Bid)
            .cloned()
            .collect();
        let mut asks: Vec<Resting> = self
            .resting
            .iter()
            .filter(|o| o.side == Side::Ask)
            .cloned()
            .collect();
        bids.sort_by_key(|o| std::cmp::Reverse(o.price));
        asks.sort_by_key(|o| o.price);

        let mut events: Vec<EventKind> = bids
            .into_iter()
            .chain(asks)
            .map(|o| EventKind::OrderCancelled {
                order: o.id,
                user: o.user,
                reason: CancelReason::MarketLocked,
                remaining: Qty::new(o.remaining),
            })
            .collect();
        self.resting.clear();
        self.state = MarketState::Locked;
        events.push(EventKind::MarketLocked);
        events
    }

    fn snapshot(&self, id: MarketId) -> BookSnapshot {
        let level = |side: Side, raw: u8| -> Option<SnapshotLevel> {
            let orders: Vec<SnapshotOrder> = self
                .resting
                .iter()
                .filter(|o| o.side == side && o.price.get() == raw)
                .map(|o| SnapshotOrder {
                    order: o.id,
                    user: o.user,
                    outcome: o.outcome,
                    action: o.action,
                    typed_price: o.typed_price,
                    side: o.side,
                    price: o.price,
                    original: Qty::new(o.original),
                    remaining: Qty::new(o.remaining),
                    tif: o.tif,
                })
                .collect();
            if orders.is_empty() {
                return None;
            }
            let total = orders.iter().map(|o| o.remaining.get()).sum();
            Some(SnapshotLevel {
                side,
                price: Price::new(raw).expect("loop price is valid"),
                total: Qty::new(total),
                orders,
            })
        };
        BookSnapshot {
            market: id,
            state: self.state,
            bids: (1..=99).rev().filter_map(|p| level(Side::Bid, p)).collect(),
            asks: (1..=99).filter_map(|p| level(Side::Ask, p)).collect(),
        }
    }
}

impl Reference {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn apply(&mut self, seq: Seq, cmd: Command) -> Vec<Event> {
        match cmd {
            Command::RegisterMarket { market } => {
                let kind = match self.markets.entry(market) {
                    Entry::Occupied(_) => reject(None, RejectReason::MarketAlreadyExists, None),
                    Entry::Vacant(slot) => {
                        slot.insert(RefMarket {
                            state: MarketState::Open,
                            resting: Vec::new(),
                            next_event_no: 1,
                        });
                        EventKind::MarketRegistered
                    }
                };
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
                let order = OrderId::new(seq.get());
                let Some(book) = self.markets.get_mut(&market) else {
                    return unknown_market(market, seq, Some(user), Some(order));
                };
                let kinds = book.place(order, user, outcome, action, price, qty, tif);
                self.emit(market, seq, kinds)
            }
            Command::CancelOrder {
                user,
                market,
                order,
            } => {
                let Some(book) = self.markets.get_mut(&market) else {
                    return unknown_market(market, seq, Some(user), Some(order));
                };
                let kinds = book.cancel(user, order);
                self.emit(market, seq, kinds)
            }
            Command::LockMarket { market } => {
                let Some(book) = self.markets.get_mut(&market) else {
                    return unknown_market(market, seq, None, None);
                };
                let kinds = book.lock();
                self.emit(market, seq, kinds)
            }
        }
    }

    pub fn book_snapshot(&self, market: MarketId) -> Option<BookSnapshot> {
        self.markets.get(&market).map(|m| m.snapshot(market))
    }

    fn emit(&mut self, market: MarketId, seq: Seq, kinds: Vec<EventKind>) -> Vec<Event> {
        let book = self
            .markets
            .get_mut(&market)
            .expect("events are only emitted for a registered market");
        kinds
            .into_iter()
            .map(|kind| {
                let event_no = EventNo::new(book.next_event_no);
                book.next_event_no += 1;
                Event {
                    market,
                    seq,
                    event_no,
                    kind,
                }
            })
            .collect()
    }
}
