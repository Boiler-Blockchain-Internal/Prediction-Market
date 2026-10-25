use boilermarket_engine::{Action, Command, Engine, MarketId, Outcome, Seq, TimeInForce, UserId};

#[test]
fn replaying_the_same_commands_produces_identical_events_and_state() {
    let market = MarketId::new(44);
    let commands = vec![
        Command::RegisterMarket { market },
        Command::PlaceOrder {
            user: UserId::new(1),
            market,
            outcome: Outcome::Yes,
            action: Action::Buy,
            price: 48,
            qty: 7,
            tif: TimeInForce::Gtc,
        },
        Command::PlaceOrder {
            user: UserId::new(2),
            market,
            outcome: Outcome::No,
            action: Action::Buy,
            price: 52,
            qty: 3,
            tif: TimeInForce::Gtc,
        },
        Command::PlaceOrder {
            user: UserId::new(3),
            market,
            outcome: Outcome::Yes,
            action: Action::Sell,
            price: 47,
            qty: 9,
            tif: TimeInForce::Ioc,
        },
        Command::LockMarket { market },
    ];
    let replay = |commands: &[Command]| {
        let mut engine = Engine::new();
        let events: Vec<_> = commands
            .iter()
            .cloned()
            .enumerate()
            .flat_map(|(i, command)| engine.apply(Seq::new(i as u64 + 1), command))
            .collect();
        (events, engine.book_snapshot(market))
    };
    assert_eq!(replay(&commands), replay(&commands));
}
