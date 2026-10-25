use std::fmt;

macro_rules! id_type {
    ($name:ident) => {
        #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(u64);

        impl $name {
            pub const fn new(value: u64) -> Self {
                Self(value)
            }

            pub const fn get(self) -> u64 {
                self.0
            }
        }

        impl From<u64> for $name {
            fn from(value: u64) -> Self {
                Self::new(value)
            }
        }
    };
}

id_type!(OrderId);
id_type!(UserId);
id_type!(MarketId);
id_type!(Seq);
id_type!(EventNo);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Price(u8);

impl Price {
    pub fn new(raw: u8) -> Result<Self, PriceError> {
        if (1..=99).contains(&raw) {
            Ok(Self(raw))
        } else {
            Err(PriceError { raw })
        }
    }

    pub const fn get(self) -> u8 {
        self.0
    }

    pub fn complement(self) -> Self {
        // A valid price is in 1..=99, so its complement is valid too.
        Self(100 - self.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PriceError {
    pub raw: u8,
}

impl fmt::Display for PriceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "price {} is outside 1..=99", self.raw)
    }
}

impl std::error::Error for PriceError {}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Qty(u64);

impl Qty {
    pub const ZERO: Self = Self(0);

    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Outcome {
    Yes,
    No,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Action {
    Buy,
    Sell,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Side {
    Bid,
    Ask,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TimeInForce {
    Gtc,
    Ioc,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MarketState {
    Open,
    Locked,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CancelReason {
    User,
    SelfTrade,
    MarketLocked,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RejectReason {
    UnknownMarket,
    MarketAlreadyExists,
    MarketNotOpen,
    BadPrice,
    BadQuantity,
    OpenOrderCap,
    OrderNotFound,
    NotOwner,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn price_boundaries_and_complement() {
        assert_eq!(Price::new(0), Err(PriceError { raw: 0 }));
        assert_eq!(Price::new(1).unwrap().get(), 1);
        assert_eq!(Price::new(99).unwrap().get(), 99);
        assert_eq!(Price::new(100), Err(PriceError { raw: 100 }));
        assert_eq!(Price::new(255), Err(PriceError { raw: 255 }));
        assert_eq!(Price::new(1).unwrap().complement().get(), 99);
        assert_eq!(Price::new(99).unwrap().complement().get(), 1);
    }
}
