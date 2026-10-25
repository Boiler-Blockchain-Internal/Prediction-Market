#![forbid(unsafe_code)]

mod command;
mod engine;
mod event;
mod level;
mod market;
mod types;

pub use command::Command;
pub use engine::Engine;
pub use event::{Event, EventKind};
pub use market::{BookSnapshot, Market, SnapshotLevel, SnapshotOrder, MAX_QTY, OPEN_ORDER_CAP};
pub use types::{
    Action, CancelReason, EventNo, MarketId, MarketState, OrderId, Outcome, Price, PriceError, Qty,
    RejectReason, Seq, Side, TimeInForce, UserId,
};
