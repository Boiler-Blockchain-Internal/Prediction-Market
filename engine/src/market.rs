use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::level::Level;
use crate::{
    Action, CancelReason, EventKind, EventNo, MarketId, MarketState, OrderId, Outcome, Price, Qty,
    RejectReason, Side, TimeInForce, UserId,
};

pub const MAX_QTY: u64 = 1_000;
pub const OPEN_ORDER_CAP: u32 = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct SlotIdx(u32);

impl SlotIdx {
    fn index(self) -> usize {
        self.0 as usize
    }
}

#[derive(Clone, Debug)]
struct OrderNode {
    id: OrderId,
    user: UserId,
    outcome: Outcome,
    action: Action,
    typed_price: Price,
    side: Side,
    price: Price,
    original: Qty,
    remaining: Qty,
    tif: TimeInForce,
    prev: Option<SlotIdx>,
    next: Option<SlotIdx>,
}

pub(crate) struct OrderRequest {
    pub(crate) order: OrderId,
    pub(crate) user: UserId,
    pub(crate) outcome: Outcome,
    pub(crate) action: Action,
    pub(crate) raw_price: u8,
    pub(crate) raw_qty: u64,
    pub(crate) tif: TimeInForce,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SnapshotOrder {
    pub order: OrderId,
    pub user: UserId,
    pub outcome: Outcome,
    pub action: Action,
    pub typed_price: Price,
    pub side: Side,
    pub price: Price,
    pub original: Qty,
    pub remaining: Qty,
    pub tif: TimeInForce,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SnapshotLevel {
    pub side: Side,
    pub price: Price,
    pub total: Qty,
    pub orders: Vec<SnapshotOrder>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BookSnapshot {
    pub market: MarketId,
    pub state: MarketState,
    /// Bids are ordered best-to-worst (99 down to 1).
    pub bids: Vec<SnapshotLevel>,
    /// Asks are ordered best-to-worst (1 up to 99).
    pub asks: Vec<SnapshotLevel>,
}

#[derive(Clone, Debug)]
pub struct Market {
    id: MarketId,
    state: MarketState,
    bids: [Level; 100],
    asks: [Level; 100],
    slab: Vec<Option<OrderNode>>,
    free: Vec<SlotIdx>,
    index: HashMap<OrderId, SlotIdx>,
    open_per_user: HashMap<UserId, u32>,
    next_event_no: EventNo,
}

impl Market {
    pub(crate) fn new(id: MarketId) -> Self {
        Self {
            id,
            state: MarketState::Open,
            bids: std::array::from_fn(|_| Level::default()),
            asks: std::array::from_fn(|_| Level::default()),
            slab: Vec::new(),
            free: Vec::new(),
            index: HashMap::new(),
            open_per_user: HashMap::new(),
            next_event_no: EventNo::new(1),
        }
    }

    pub(crate) fn take_event_no(&mut self) -> EventNo {
        let current = self.next_event_no;
        self.next_event_no = EventNo::new(current.get() + 1);
        current
    }

    pub(crate) fn place_order(&mut self, request: OrderRequest) -> Vec<EventKind> {
        let OrderRequest {
            order,
            user,
            outcome,
            action,
            raw_price,
            raw_qty,
            tif,
        } = request;
        if self.state != MarketState::Open {
            return vec![Self::rejection(
                Some(user),
                RejectReason::MarketNotOpen,
                Some(order),
            )];
        }

        let typed_price = match Price::new(raw_price) {
            Ok(price) => price,
            Err(_) => {
                return vec![Self::rejection(
                    Some(user),
                    RejectReason::BadPrice,
                    Some(order),
                )]
            }
        };

        if !(1..=MAX_QTY).contains(&raw_qty) {
            return vec![Self::rejection(
                Some(user),
                RejectReason::BadQuantity,
                Some(order),
            )];
        }

        if self.open_per_user.get(&user).copied().unwrap_or(0) >= OPEN_ORDER_CAP {
            return vec![Self::rejection(
                Some(user),
                RejectReason::OpenOrderCap,
                Some(order),
            )];
        }

        let qty = Qty::new(raw_qty);
        let (side, price) = translate(outcome, action, typed_price);
        let mut incoming_remaining = qty;
        let mut events = vec![EventKind::OrderAccepted {
            order,
            user,
            outcome,
            action,
            typed_price,
            side,
            price,
            qty,
            tif,
        }];

        while incoming_remaining.get() > 0 {
            let Some((maker_side, maker_price, maker_slot)) = self.best_crossing(side, price)
            else {
                break;
            };

            let maker = self.node(maker_slot);
            let maker_id = maker.id;
            let maker_user = maker.user;
            let maker_remaining = maker.remaining;

            if maker_user == user {
                let removed = self.unlink(maker_side, maker_price, maker_slot);
                self.decrement_open(removed.user);
                events.push(EventKind::OrderCancelled {
                    order: removed.id,
                    user: removed.user,
                    reason: CancelReason::SelfTrade,
                    remaining: removed.remaining,
                });
                continue;
            }

            let fill = Qty::new(incoming_remaining.get().min(maker_remaining.get()));
            incoming_remaining = Qty::new(incoming_remaining.get() - fill.get());
            self.reduce_resting(maker_side, maker_price, maker_slot, fill);
            events.push(EventKind::TradeExecuted {
                maker: maker_id,
                taker: order,
                maker_user,
                taker_user: user,
                price: maker_price,
                qty: fill,
                taker_side: side,
            });

            if self.node(maker_slot).remaining == Qty::ZERO {
                let filled = self.pop_front(maker_side, maker_price);
                self.decrement_open(filled.user);
                events.push(EventKind::OrderFilled {
                    order: filled.id,
                    user: filled.user,
                });
            }
        }

        if incoming_remaining == Qty::ZERO {
            events.push(EventKind::OrderFilled { order, user });
        } else if tif == TimeInForce::Gtc {
            let node = OrderNode {
                id: order,
                user,
                outcome,
                action,
                typed_price,
                side,
                price,
                original: qty,
                remaining: incoming_remaining,
                tif,
                prev: None,
                next: None,
            };
            let slot = self.allocate(node);
            self.push_back(side, price, slot);
            *self.open_per_user.entry(user).or_insert(0) += 1;
            events.push(EventKind::OrderRested {
                order,
                user,
                side,
                price,
                remaining: incoming_remaining,
            });
        } else {
            events.push(EventKind::OrderExpired {
                order,
                user,
                unfilled: incoming_remaining,
            });
        }

        events
    }

    pub(crate) fn cancel_order(&mut self, user: UserId, order: OrderId) -> Vec<EventKind> {
        let Some(&slot) = self.index.get(&order) else {
            return vec![Self::rejection(
                Some(user),
                RejectReason::OrderNotFound,
                Some(order),
            )];
        };
        let node = self.node(slot);
        if node.user != user {
            return vec![Self::rejection(
                Some(user),
                RejectReason::NotOwner,
                Some(order),
            )];
        }
        let removed = self.unlink(node.side, node.price, slot);
        self.decrement_open(removed.user);
        vec![EventKind::OrderCancelled {
            order: removed.id,
            user: removed.user,
            reason: CancelReason::User,
            remaining: removed.remaining,
        }]
    }

    pub(crate) fn lock(&mut self) -> Vec<EventKind> {
        if self.state != MarketState::Open {
            return vec![Self::rejection(None, RejectReason::MarketNotOpen, None)];
        }

        let mut events = Vec::new();
        for raw_price in (1..=99).rev() {
            let price = Price::new(raw_price).expect("loop only contains valid prices");
            while let Some(slot) = self.level(Side::Bid, price).head {
                let removed = self.unlink(Side::Bid, price, slot);
                self.decrement_open(removed.user);
                events.push(lock_cancel(&removed));
            }
        }
        for raw_price in 1..=99 {
            let price = Price::new(raw_price).expect("loop only contains valid prices");
            while let Some(slot) = self.level(Side::Ask, price).head {
                let removed = self.unlink(Side::Ask, price, slot);
                self.decrement_open(removed.user);
                events.push(lock_cancel(&removed));
            }
        }
        self.state = MarketState::Locked;
        events.push(EventKind::MarketLocked);
        events
    }

    pub(crate) fn best_bid(&self) -> Option<Price> {
        (1..=99)
            .rev()
            .find(|&price| self.bids[price].count > 0)
            .map(|price| Price::new(price as u8).expect("price index is valid"))
    }

    pub(crate) fn best_ask(&self) -> Option<Price> {
        (1..=99)
            .find(|&price| self.asks[price].count > 0)
            .map(|price| Price::new(price as u8).expect("price index is valid"))
    }

    pub(crate) fn snapshot(&self) -> BookSnapshot {
        let bids = (1..=99)
            .rev()
            .filter_map(|raw| self.snapshot_level(Side::Bid, raw))
            .collect();
        let asks = (1..=99)
            .filter_map(|raw| self.snapshot_level(Side::Ask, raw))
            .collect();
        BookSnapshot {
            market: self.id,
            state: self.state,
            bids,
            asks,
        }
    }

    fn snapshot_level(&self, side: Side, raw_price: usize) -> Option<SnapshotLevel> {
        let price = Price::new(raw_price as u8).expect("price index is valid");
        let level = self.level(side, price);
        if level.count == 0 {
            return None;
        }
        let mut orders = Vec::with_capacity(level.count as usize);
        let mut cursor = level.head;
        while let Some(slot) = cursor {
            let node = self.node(slot);
            orders.push(SnapshotOrder {
                order: node.id,
                user: node.user,
                outcome: node.outcome,
                action: node.action,
                typed_price: node.typed_price,
                side: node.side,
                price: node.price,
                original: node.original,
                remaining: node.remaining,
                tif: node.tif,
            });
            cursor = node.next;
        }
        Some(SnapshotLevel {
            side,
            price,
            total: level.total,
            orders,
        })
    }

    fn rejection(user: Option<UserId>, reason: RejectReason, order: Option<OrderId>) -> EventKind {
        EventKind::OrderRejected {
            user,
            reason,
            order,
        }
    }

    fn best_crossing(&self, incoming_side: Side, limit: Price) -> Option<(Side, Price, SlotIdx)> {
        let maker_side = match incoming_side {
            Side::Bid => Side::Ask,
            Side::Ask => Side::Bid,
        };
        let maker_price = match incoming_side {
            Side::Bid => self.best_ask().filter(|price| *price <= limit),
            Side::Ask => self.best_bid().filter(|price| *price >= limit),
        }?;
        let head = self
            .level(maker_side, maker_price)
            .head
            .expect("nonempty best level must have a head");
        Some((maker_side, maker_price, head))
    }

    fn allocate(&mut self, node: OrderNode) -> SlotIdx {
        if let Some(slot) = self.free.pop() {
            debug_assert!(self.slab[slot.index()].is_none());
            self.slab[slot.index()] = Some(node);
            slot
        } else {
            let index = self.slab.len();
            assert!(
                index <= u32::MAX as usize,
                "market slab exhausted u32 slots"
            );
            self.slab.push(Some(node));
            SlotIdx(index as u32)
        }
    }

    /// The only operation that links a new node into a level.
    fn push_back(&mut self, side: Side, price: Price, slot: SlotIdx) {
        let tail = self.level(side, price).tail;
        {
            let node = self.node_mut(slot);
            node.prev = tail;
            node.next = None;
        }
        if let Some(tail_slot) = tail {
            self.node_mut(tail_slot).next = Some(slot);
        } else {
            self.level_mut(side, price).head = Some(slot);
        }
        let remaining = self.node(slot).remaining;
        let id = self.node(slot).id;
        let level = self.level_mut(side, price);
        level.tail = Some(slot);
        level.total = Qty::new(level.total.get() + remaining.get());
        level.count += 1;
        let replaced = self.index.insert(id, slot);
        assert!(replaced.is_none(), "duplicate live order id {id:?}");
    }

    /// The only operation that removes a node or rewires existing links.
    fn unlink(&mut self, side: Side, price: Price, slot: SlotIdx) -> OrderNode {
        let (prev, next, remaining, id) = {
            let node = self.node(slot);
            (node.prev, node.next, node.remaining, node.id)
        };

        if let Some(prev_slot) = prev {
            self.node_mut(prev_slot).next = next;
        } else {
            self.level_mut(side, price).head = next;
        }
        if let Some(next_slot) = next {
            self.node_mut(next_slot).prev = prev;
        } else {
            self.level_mut(side, price).tail = prev;
        }

        let level = self.level_mut(side, price);
        level.total = Qty::new(level.total.get() - remaining.get());
        level.count -= 1;
        let indexed = self.index.remove(&id);
        assert_eq!(indexed, Some(slot), "order index disagreed with slab");
        let removed = self.slab[slot.index()]
            .take()
            .expect("cannot unlink an empty slab slot");
        self.free.push(slot);
        removed
    }

    /// Removes the oldest node from a level through `unlink`.
    fn pop_front(&mut self, side: Side, price: Price) -> OrderNode {
        let head = self
            .level(side, price)
            .head
            .expect("cannot pop an empty price level");
        self.unlink(side, price, head)
    }

    fn reduce_resting(&mut self, side: Side, price: Price, slot: SlotIdx, fill: Qty) {
        let remaining = self.node(slot).remaining;
        assert!(fill <= remaining, "fill exceeds resting order remainder");
        self.node_mut(slot).remaining = Qty::new(remaining.get() - fill.get());
        let level = self.level_mut(side, price);
        level.total = Qty::new(level.total.get() - fill.get());
    }

    fn decrement_open(&mut self, user: UserId) {
        let count = self
            .open_per_user
            .get_mut(&user)
            .expect("resting order owner missing from open counts");
        assert!(*count > 0, "open order count underflow");
        *count -= 1;
        if *count == 0 {
            self.open_per_user.remove(&user);
        }
    }

    fn level(&self, side: Side, price: Price) -> &Level {
        match side {
            Side::Bid => &self.bids[price.get() as usize],
            Side::Ask => &self.asks[price.get() as usize],
        }
    }

    fn level_mut(&mut self, side: Side, price: Price) -> &mut Level {
        match side {
            Side::Bid => &mut self.bids[price.get() as usize],
            Side::Ask => &mut self.asks[price.get() as usize],
        }
    }

    fn node(&self, slot: SlotIdx) -> &OrderNode {
        self.slab
            .get(slot.index())
            .and_then(Option::as_ref)
            .expect("live slot must contain an order")
    }

    fn node_mut(&mut self, slot: SlotIdx) -> &mut OrderNode {
        self.slab
            .get_mut(slot.index())
            .and_then(Option::as_mut)
            .expect("live slot must contain an order")
    }

    #[cfg(any(test, debug_assertions))]
    pub(crate) fn debug_check(&self) {
        self.check_invariants();
    }

    #[cfg(not(any(test, debug_assertions)))]
    pub(crate) fn debug_check(&self) {}

    /// Always compiled so tests can check invariants in any build profile (including
    /// `cargo test --release`). `debug_check` below is what the engine calls on every command.
    pub(crate) fn check_invariants(&self) {
        if let (Some(bid), Some(ask)) = (self.best_bid(), self.best_ask()) {
            assert!(bid < ask, "book is crossed: bid {bid:?}, ask {ask:?}");
        }

        let mut reached = BTreeSet::new();
        let mut expected_open = BTreeMap::<UserId, u32>::new();
        for side in [Side::Bid, Side::Ask] {
            for raw_price in 1..=99 {
                let price = Price::new(raw_price).expect("loop price is valid");
                let level = self.level(side, price);
                assert_eq!(level.head.is_some(), level.tail.is_some());
                if let Some(head) = level.head {
                    assert!(self.node(head).prev.is_none(), "head has a predecessor");
                }
                if let Some(tail) = level.tail {
                    assert!(self.node(tail).next.is_none(), "tail has a successor");
                }

                let mut forward = Vec::new();
                let mut total = 0_u64;
                let mut previous = None;
                let mut cursor = level.head;
                while let Some(slot) = cursor {
                    assert!(reached.insert(slot), "slot appears in multiple lists");
                    let node = self.node(slot);
                    assert_eq!(node.prev, previous, "back link disagrees");
                    assert_eq!(node.side, side, "node is on the wrong side");
                    assert_eq!(node.price, price, "node is on the wrong price level");
                    assert!(node.remaining > Qty::ZERO);
                    assert!(node.remaining <= node.original);
                    assert_eq!(node.tif, TimeInForce::Gtc);
                    assert_eq!(self.index.get(&node.id), Some(&slot));
                    total += node.remaining.get();
                    *expected_open.entry(node.user).or_insert(0) += 1;
                    forward.push(slot);
                    previous = Some(slot);
                    cursor = node.next;
                }
                assert_eq!(previous, level.tail);
                assert_eq!(forward.len(), level.count as usize);
                assert_eq!(total, level.total.get());

                let mut reverse = Vec::new();
                let mut next = None;
                cursor = level.tail;
                while let Some(slot) = cursor {
                    let node = self.node(slot);
                    assert_eq!(node.next, next, "forward link disagrees");
                    reverse.push(slot);
                    next = Some(slot);
                    cursor = node.prev;
                }
                reverse.reverse();
                assert_eq!(forward, reverse, "forward and reverse walks differ");
            }
        }

        assert_eq!(self.index.len(), reached.len());
        for (raw_slot, entry) in self.slab.iter().enumerate() {
            let slot = SlotIdx(raw_slot as u32);
            assert_eq!(entry.is_some(), reached.contains(&slot));
        }
        let mut free_seen = BTreeSet::new();
        for &slot in &self.free {
            assert!(
                free_seen.insert(slot),
                "free list contains a duplicate slot"
            );
            assert!(self.slab[slot.index()].is_none());
            assert!(!reached.contains(&slot));
        }
        assert_eq!(self.slab.len(), reached.len() + self.free.len());
        assert_eq!(self.open_per_user.len(), expected_open.len());
        for (user, count) in expected_open {
            assert_eq!(self.open_per_user.get(&user), Some(&count));
        }
    }
}

fn translate(outcome: Outcome, action: Action, typed_price: Price) -> (Side, Price) {
    match (outcome, action) {
        (Outcome::Yes, Action::Buy) => (Side::Bid, typed_price),
        (Outcome::Yes, Action::Sell) => (Side::Ask, typed_price),
        (Outcome::No, Action::Buy) => (Side::Ask, typed_price.complement()),
        (Outcome::No, Action::Sell) => (Side::Bid, typed_price.complement()),
    }
}

fn lock_cancel(node: &OrderNode) -> EventKind {
    EventKind::OrderCancelled {
        order: node.id,
        user: node.user,
        reason: CancelReason::MarketLocked,
        remaining: node.remaining,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(id: u64, user: u64, side: Side, price: u8, qty: u64) -> OrderNode {
        let price = Price::new(price).unwrap();
        OrderNode {
            id: OrderId::new(id),
            user: UserId::new(user),
            outcome: Outcome::Yes,
            action: if side == Side::Bid {
                Action::Buy
            } else {
                Action::Sell
            },
            typed_price: price,
            side,
            price,
            original: Qty::new(qty),
            remaining: Qty::new(qty),
            tif: TimeInForce::Gtc,
            prev: None,
            next: None,
        }
    }

    #[test]
    fn list_operations_cover_middle_head_tail_empty_and_lifo_reuse() {
        let mut market = Market::new(MarketId::new(1));
        let price = Price::new(50).unwrap();
        let mut slots = Vec::new();
        for id in 1..=5 {
            let slot = market.allocate(node(id, id, Side::Bid, 50, id));
            market.push_back(Side::Bid, price, slot);
            *market.open_per_user.entry(UserId::new(id)).or_insert(0) += 1;
            slots.push(slot);
            market.check_invariants();
        }

        let middle = market.unlink(Side::Bid, price, slots[2]);
        market.decrement_open(middle.user);
        market.check_invariants();
        let front = market.pop_front(Side::Bid, price);
        market.decrement_open(front.user);
        market.check_invariants();
        let head = market.unlink(Side::Bid, price, slots[1]);
        market.decrement_open(head.user);
        market.check_invariants();
        let tail = market.unlink(Side::Bid, price, slots[4]);
        market.decrement_open(tail.user);
        market.check_invariants();
        let last = market.unlink(Side::Bid, price, slots[3]);
        market.decrement_open(last.user);
        market.check_invariants();

        let expected_reuse = slots[3];
        let reused = market.allocate(node(6, 6, Side::Bid, 50, 6));
        assert_eq!(reused, expected_reuse);
        market.push_back(Side::Bid, price, reused);
        *market.open_per_user.entry(UserId::new(6)).or_insert(0) += 1;
        market.check_invariants();
    }
}
