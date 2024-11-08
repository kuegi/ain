use ain_macros::ConsensusEncoding;
use bitcoin::io;

use super::common::CompactVec;

#[derive(ConsensusEncoding, Debug, PartialEq, Eq, Clone)]
pub struct CurrencyPair {
    pub token: String,
    pub currency: String,
}

#[derive(ConsensusEncoding, Debug, PartialEq, Eq, Clone)]
pub struct TokenAmount {
    pub currency: String,
    pub amount: i64,
}

#[derive(ConsensusEncoding, Debug, PartialEq, Eq, Clone)]
pub struct TokenPrice {
    pub token: String,
    pub prices: CompactVec<TokenAmount>,
}
