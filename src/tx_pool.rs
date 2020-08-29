use common_types::transaction::SignedTransaction;
use std::collections::HashSet;
use ethereum_types::H256;

type Tx = SignedTransaction;

#[derive(Default)]
pub struct TxPool {
    pub txs: Vec<Tx>,
    pub ids: HashSet<H256>,
}

impl TxPool {
    pub fn add(&mut self, tx: Tx) -> bool{
        let tid = tx.hash();
        if !self.ids.contains(&tid) {
            self.txs.push(tx);
            self.ids.insert(tid);
            self.txs.sort_by_key(|t| t.nonce);
            true
        } else {
            false
        }
    }
}

// TODO if the tx-pool becomes very large, it should be cleaned. (free allocated memory)
// TODO if a tx in the tx-pool, but not in the chain, it should be processed.
// TODO if a tx is in chain now, it should not be put into the tx-pool.


