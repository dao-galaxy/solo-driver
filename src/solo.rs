
use std::{thread, time};
use std::sync::mpsc::{Sender, Receiver};
use common_types::header::Header;
use common_types::transaction::{UnverifiedTransaction, SignedTransaction};
use std::sync::{Mutex, Arc};
use common_types::ipc::*;
use log::{debug, error};
use ethereum_types::{H256, U256, Address};
use common_types::block::Block;
use std::collections::{HashMap, HashSet};
use parity_crypto::publickey::{ExtendedKeyPair, Derivation, KeyPair, public_to_address};
use rlp::Rlp;
use crate::tx_pool::TxPool;


pub struct SoloDriver {
    pub my_peer_index: usize,
    pub my_peer_id: [u8; 1],
    pub current_block_header: Option<Header>,
    pub new_block_header: Option<Header>,
    pub new_block_txs: Option<Vec<UnverifiedTransaction>>,
    pub chain_socket: zmq::Socket,
    pub rpc_socket: zmq::Socket,
    pub tx_pool: Arc<Mutex<TxPool>>,
    pub block_duration: u64,
}

impl SoloDriver {

    pub fn initialize(
        context: zmq::Context,
        tx_pool: Arc<Mutex<TxPool>>,
        chain_socket_str: &str,
        rpc_socket_str: &str,
        block_duration: u64,
    ) -> Self {
        let my_peer_index = 0;
        let my_peer_id = [0 as u8];

        let chain_socket = context.socket(zmq::DEALER).unwrap();
        chain_socket.set_identity(&my_peer_id).unwrap();
        chain_socket.connect(chain_socket_str).unwrap();

        let rpc_socket = context.socket(zmq::ROUTER).unwrap();
        rpc_socket.bind(rpc_socket_str).unwrap();

        let solo_model: SoloDriver = SoloDriver {
            my_peer_index,
            my_peer_id,
            current_block_header: None,
            new_block_header: None,
            new_block_txs: None,
            chain_socket,
            rpc_socket,
            tx_pool,
            block_duration
        };
        solo_model
    }

    pub fn start_server_solo(&mut self) {
        let mut sleep_sec = self.block_duration;
        let mut pool_items = vec![];
        pool_items.push(self.rpc_socket.as_poll_item(zmq::POLLIN));

        loop {
            zmq::poll(&mut pool_items, 5).unwrap();
            if pool_items[0].is_readable() {
                self.handle_rpc_request(&self.rpc_socket, &self.tx_pool);
            }
            thread::sleep(time::Duration::from_secs(sleep_sec as u64));
            if let Some(block) = self.create_block_header() {
                let block_rlp = rlp::encode(&block);
                self.new_block_header = Some(block.header);
                self.new_block_txs = Some(block.transactions);

                if self.apply_block() {
                    self.delete_commit_txs();
                    self.current_block_header = self.new_block_header.clone();
                    self.new_block_header = None;
                    self.new_block_txs = None;
                }
                sleep_sec = self.block_duration;
            } else {
                sleep_sec = 1;
            }
        }
    }

    pub fn apply_block(&self) -> bool {
        debug!("start apply block");

        let request = IpcRequest {
            method: "ApplyBlock".into(),
            id: 3,
            params: rlp::encode(&ApplyBlockReq(
                self.new_block_header.clone().unwrap(),
                self.new_block_txs.clone().unwrap(),
            )),
        };

        let reply = request_chain(&self.chain_socket, request);
        let res: ApplyBlockResp = rlp::decode(&reply.result).unwrap();
        let is_ok = res.0;
        debug!("apply block is_ok: {}", is_ok);
        is_ok
    }

    pub fn delete_commit_txs(&self) {
        let txs = self.new_block_txs.as_ref().unwrap();

        let mut tx_pool = self.tx_pool.lock().unwrap();
        let mut select_tx_indexs = vec![];
        for (i, tx) in tx_pool.txs.iter().enumerate() {
            if txs.contains(tx) {
                select_tx_indexs.push(i);
            }
        }
        select_tx_indexs.reverse();
        for i in select_tx_indexs {
            let tx_hash = &tx_pool.txs[i].hash();
            tx_pool.ids.remove(tx_hash);
            tx_pool.txs.remove(i);
        }
    }

    pub fn init_blockchain_info(&mut self) {
        debug!("start get lastblock");

        let request = IpcRequest {
            method: "LatestBlocks".into(),
            id: 1,
            params: rlp::encode(&LatestBlocksReq(1)),
        };

        let reply = request_chain(&self.chain_socket, request);
        let res: LatestBlocksResp = rlp::decode(&reply.result).unwrap();
        let lastblock = res.0.get(0).unwrap().clone();
        debug!("receive lastblock: {:?}", lastblock);
        self.current_block_header = Some(lastblock);
    }

    pub fn create_block_header(&self) -> Option<Block> {
        let txs = self.select_txs();

        if txs.is_empty() {
            return None;
        }

        let create_block_header_request = CreateHeaderReq {
            parent_block_hash: self.current_block_header.clone().unwrap().hash(),
            author: self.my_address(),
            extra_data: vec![],
            gas_limit: U256::from(1000),
            difficulty: U256::from(0),
            transactions: txs.clone(),
        };
        debug!(
            "start create_block_header_request: {:?}",
            create_block_header_request
        );
        let request = IpcRequest {
            method: "CreateHeader".into(),
            id: 2,
            params: rlp::encode(&create_block_header_request),
        };

        let reply = request_chain(&self.chain_socket, request);
        let res: CreateHeaderResp = rlp::decode(&reply.result).unwrap();
        let new_block_header = res.0;
        debug!("receive new_block_header: {:?}", new_block_header);
        Some(Block {
            header: new_block_header,
            transactions: txs,
        })
    }

    pub fn select_txs(&self) -> Vec<UnverifiedTransaction> {
        let mut tx_pool = self.tx_pool.lock().unwrap();
        debug!("tx_pool count: {}, {}", tx_pool.txs.len(), tx_pool.ids.len());
        let mut account_infos = HashMap::new();
        for tx in &tx_pool.txs {
            if !account_infos.contains_key(&tx.sender()) {
                account_infos.insert(tx.sender(), query_account_info(&self.chain_socket, &tx.sender()));
            }
        }

        let mut res = vec![];
        // let mut select_senders = HashSet::new();
        let mut should_delete_txs = vec![];

        for (i, tx) in tx_pool.txs.iter().enumerate() {
            // if select_senders.contains(&tx.sender())
            // ||
            if tx.nonce != account_infos[&tx.sender()].0 {
                should_delete_txs.push(i);
                continue;
            }
            if account_infos[&tx.sender()].1 < tx.gas * tx.gas_price {
                should_delete_txs.push(i);
                continue;
            }

            // select_senders.insert(tx.sender());

            res.push(tx.clone().into());

            account_infos.get_mut(&tx.sender()).unwrap().0 += U256::from(1);

            if res.len() >= 5 {
                break;
            }
        }

        should_delete_txs.reverse();
        for i in should_delete_txs {
            let tx_hash = &tx_pool.txs[i].hash();
            tx_pool.ids.remove(tx_hash);
            tx_pool.txs.remove(i);
        }

        res
    }

    pub fn my_address(&self) -> Address {
        let ek = ExtendedKeyPair::with_seed(b"sawtooth-pbft-demo").unwrap();
        let my_ek = ek
            .derive(Derivation::Soft(self.my_peer_id[0] as u32))
            .unwrap();
        let my_kp = KeyPair::from_secret(my_ek.secret().as_raw().clone()).unwrap();
        assert_eq!(my_kp.public(), my_ek.public().public());
        let address = public_to_address(my_ek.public().public());
        assert_eq!(address, my_kp.address());
        address
    }

    pub fn start_server(&mut self) {
        self.init_blockchain_info();
        self.start_server_solo();
    }


    pub fn handle_rpc_request(&self, rpc_socket: &zmq::Socket, tx_pool: &Arc<Mutex<TxPool>>) {
        let mut received_parts = rpc_socket.recv_multipart(0).unwrap();
        let msg_bytes = received_parts.pop().unwrap();
        let zmq_identity = received_parts.pop().unwrap();

        let request = rlp::decode(&msg_bytes);
        if request.is_err() {
            return;
        }
        let request: IpcRequest = request.unwrap();

        match request.method.as_str() {
            "SendToTxPool" => {
                debug!("receive rpc request: {:?}", request);
                let r = Rlp::new(&request.params).as_list();
                if r.is_err() {
                    return;
                }

                let txs: Vec<UnverifiedTransaction> = r.unwrap();
                debug!("rpc request txs: {:?}", txs);

                let mut tp = tx_pool.lock().unwrap();
                let mut has_new_txs = false;
                for tx in txs {
                    if let Ok(vtx) = tx.verify_unordered() {
                        if tp.add(vtx) {
                            has_new_txs = true;
                        }
                    }
                }

                if has_new_txs {
                    // TODO something
                }

                let res = IpcReply {
                    id: request.id,
                    result: rlp::encode(&b"ok".to_vec()),
                };
                rpc_socket.send_multipart(vec![zmq_identity, rlp::encode(&res)], 0).unwrap();
            }
            _ => {
                error!("receive rpc request: {:?}", request);
            }
        }
    }

}


