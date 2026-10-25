use crate::market::SlotIdx;
use crate::Qty;

#[derive(Clone, Debug, Default)]
pub(crate) struct Level {
    pub(crate) head: Option<SlotIdx>,
    pub(crate) tail: Option<SlotIdx>,
    pub(crate) total: Qty,
    pub(crate) count: u32,
}
