//! Shared random-command generator for the property and differential tests.
//!
//! Streams mix every command the engine understands, including deliberately bad ones:
//! unknown markets, duplicate registrations, out-of-range prices and sizes, cancels of
//! orders that do not exist or belong to someone else, and locks at arbitrary points.
//! Markets 1-3 are registered up front; market 4 is never registered unless a random
//! `Register` hits it, so unknown-market paths get exercised too.

use boilermarket_engine::{Action, Command, MarketId, OrderId, Outcome, TimeInForce, UserId};
use proptest::prelude::*;

#[derive(Clone, Debug)]
pub enum Op {
    Register {
        market: u64,
    },
    Place {
        market: u64,
        user: u64,
        yes: bool,
        buy: bool,
        price: u8,
        qty: u64,
        gtc: bool,
    },
    Cancel {
        market: u64,
        user: u64,
        pick: usize,
    },
    Lock {
        market: u64,
    },
}

/// Mostly prices near the middle so orders cross often, with occasional out-of-range values.
fn price() -> impl Strategy<Value = u8> {
    prop_oneof![
        20 => 40_u8..=60,
        2 => 1_u8..=99,
        1 => Just(0_u8),
        1 => Just(100_u8),
    ]
}

/// Mostly small sizes, with occasional zero and over-limit sizes.
fn qty() -> impl Strategy<Value = u64> {
    prop_oneof![
        20 => 1_u64..=15,
        1 => Just(0_u64),
        1 => Just(1_001_u64),
    ]
}

pub fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        1 => (1_u64..=4).prop_map(|market| Op::Register { market }),
        14 => (
            1_u64..=4,
            1_u64..=4,
            any::<bool>(),
            any::<bool>(),
            price(),
            qty(),
            any::<bool>(),
        )
            .prop_map(|(market, user, yes, buy, price, qty, gtc)| Op::Place {
                market,
                user,
                yes,
                buy,
                price,
                qty,
                gtc,
            }),
        5 => (1_u64..=4, 1_u64..=4, any::<usize>())
            .prop_map(|(market, user, pick)| Op::Cancel { market, user, pick }),
        1 => (1_u64..=4).prop_map(|market| Op::Lock { market }),
    ]
}

pub fn stream() -> impl Strategy<Value = Vec<Op>> {
    prop::collection::vec(op(), 0..120)
}

/// Turns ops into commands. Command `i` is applied with sequence number `i + 1`, and a
/// `PlaceOrder`'s order id is its sequence number, so cancels can aim at real orders.
pub fn commands(ops: &[Op]) -> Vec<Command> {
    let mut commands: Vec<Command> = (1..=3)
        .map(|market| Command::RegisterMarket {
            market: MarketId::new(market),
        })
        .collect();
    // (order id, market, user) of every PlaceOrder issued so far.
    let mut placed: Vec<(u64, u64, u64)> = Vec::new();

    for op in ops {
        let seq = commands.len() as u64 + 1;
        let command = match *op {
            Op::Register { market } => Command::RegisterMarket {
                market: MarketId::new(market),
            },
            Op::Place {
                market,
                user,
                yes,
                buy,
                price,
                qty,
                gtc,
            } => {
                placed.push((seq, market, user));
                Command::PlaceOrder {
                    user: UserId::new(user),
                    market: MarketId::new(market),
                    outcome: if yes { Outcome::Yes } else { Outcome::No },
                    action: if buy { Action::Buy } else { Action::Sell },
                    price,
                    qty,
                    tif: if gtc {
                        TimeInForce::Gtc
                    } else {
                        TimeInForce::Ioc
                    },
                }
            }
            Op::Cancel { market, user, pick } => {
                if placed.is_empty() || pick % 11 == 0 {
                    // An order id that was never issued.
                    Command::CancelOrder {
                        user: UserId::new(user),
                        market: MarketId::new(market),
                        order: OrderId::new(9_000 + (pick % 50) as u64),
                    }
                } else {
                    let (order, order_market, order_user) = placed[pick % placed.len()];
                    // Usually aim at the order's real market and owner so many cancels land;
                    // sometimes use the random ones to exercise NotOwner and wrong-market.
                    let market = if pick % 5 == 0 { market } else { order_market };
                    let user = if (pick / 7) % 3 == 0 {
                        user
                    } else {
                        order_user
                    };
                    Command::CancelOrder {
                        user: UserId::new(user),
                        market: MarketId::new(market),
                        order: OrderId::new(order),
                    }
                }
            }
            Op::Lock { market } => Command::LockMarket {
                market: MarketId::new(market),
            },
        };
        commands.push(command);
    }
    commands
}
