use crate::{Action, MarketId, OrderId, Outcome, TimeInForce, UserId};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    RegisterMarket {
        market: MarketId,
    },
    PlaceOrder {
        user: UserId,
        market: MarketId,
        outcome: Outcome,
        action: Action,
        price: u8,
        qty: u64,
        tif: TimeInForce,
    },
    CancelOrder {
        user: UserId,
        market: MarketId,
        order: OrderId,
    },
    LockMarket {
        market: MarketId,
    },
}

impl Command {
    pub const fn market(&self) -> MarketId {
        match *self {
            Self::RegisterMarket { market }
            | Self::PlaceOrder { market, .. }
            | Self::CancelOrder { market, .. }
            | Self::LockMarket { market } => market,
        }
    }
}
