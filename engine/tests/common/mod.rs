use boilermarket_engine::{
    Action, Command, Engine, Event, MarketId, Outcome, Seq, TimeInForce, UserId,
};

pub const MARKET: MarketId = MarketId::new(7);

pub fn registered_engine() -> Engine {
    let mut engine = Engine::new();
    engine.apply(Seq::new(1), Command::RegisterMarket { market: MARKET });
    engine
}

// Keeping scenarios as compact rows makes the price/quantity sequences easier to audit.
#[allow(clippy::too_many_arguments)]
pub fn place(
    engine: &mut Engine,
    seq: u64,
    user: u64,
    outcome: Outcome,
    action: Action,
    price: u8,
    qty: u64,
    tif: TimeInForce,
) -> Vec<Event> {
    engine.apply(
        Seq::new(seq),
        Command::PlaceOrder {
            user: UserId::new(user),
            market: MARKET,
            outcome,
            action,
            price,
            qty,
            tif,
        },
    )
}
