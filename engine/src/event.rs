use crate::{
    Action, CancelReason, EventNo, MarketId, OrderId, Outcome, Price, Qty, RejectReason, Seq, Side,
    TimeInForce, UserId,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Event {
    pub market: MarketId,
    pub seq: Seq,
    pub event_no: EventNo,
    pub kind: EventKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EventKind {
    MarketRegistered,
    OrderAccepted {
        order: OrderId,
        user: UserId,
        outcome: Outcome,
        action: Action,
        typed_price: Price,
        side: Side,
        price: Price,
        qty: Qty,
        tif: TimeInForce,
    },
    OrderRejected {
        user: Option<UserId>,
        reason: RejectReason,
        order: Option<OrderId>,
    },
    TradeExecuted {
        maker: OrderId,
        taker: OrderId,
        maker_user: UserId,
        taker_user: UserId,
        price: Price,
        qty: Qty,
        taker_side: Side,
    },
    OrderFilled {
        order: OrderId,
        user: UserId,
    },
    OrderRested {
        order: OrderId,
        user: UserId,
        side: Side,
        price: Price,
        remaining: Qty,
    },
    OrderCancelled {
        order: OrderId,
        user: UserId,
        reason: CancelReason,
        remaining: Qty,
    },
    OrderExpired {
        order: OrderId,
        user: UserId,
        unfilled: Qty,
    },
    MarketLocked,
}
