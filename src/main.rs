
mod solo;
mod tx_pool;
mod config;

use env_logger;
use log::{info, debug};
use std::env;
use std::sync::{Arc, Mutex};
use std::thread;
use clap::{App, load_yaml, ArgMatches, Arg};
use std::str::FromStr;

use crate::solo::SoloDriver;
use crate::tx_pool::TxPool;
use crate::config::{read_config_file, parse_config_string};
use common_types::ipc::*;


fn main() {
    let yaml = load_yaml!("clap.yaml");  // src/clap.yaml
    let matches = App::from(yaml).get_matches();

    let config_file = matches.value_of("config").unwrap_or("src/bloom.conf");
    // target/debug/bloom-solo -c src/bloom.conf
    let toml_string = read_config_file(config_file);
    let decoded_config = parse_config_string(toml_string.as_str());
    let decoded_config_clone = decoded_config.clone();

    let log_level = matches.value_of("log").unwrap_or(
        &decoded_config.log_level.unwrap_or("debug".to_string())
    ).to_string();

    env::set_var("RUST_LOG", log_level.as_str());
    env_logger::init();
    info!("log level: {:?}", log_level.as_str());
    info!("{:#?}", matches);
    info!("{:#?}", decoded_config_clone);

    let consensus = decoded_config.consensus.unwrap_or("solo".to_string());
    let mut peers_count = 0;
    let my_peer_index = decoded_config.index.unwrap_or(0);

    let block_duration = decoded_config.block_duration.unwrap_or(5);

    let chain_socket = decoded_config.chain_socket.unwrap_or("tcp://127.0.0.1:8050".to_string());
    let rpc_socket = decoded_config.rpc_socket.unwrap_or("tcp://127.0.0.1:7050".to_string());

    info!("my peer index: {:?}", my_peer_index);
    info!("block duration (period, time interval): {:?}", block_duration);
    info!("chain socket: {:?}", chain_socket);
    info!("rpc socket: {:?}", rpc_socket);

    let tx_pool = Arc::new(Mutex::new(TxPool::default()));
    let context = zmq::Context::new();

    match consensus.as_str() {
        "solo" => {
            let mut driver = solo::SoloDriver::initialize(
                context.clone(),
                tx_pool.clone(),
                chain_socket.as_str(),
                rpc_socket.as_str(),
                block_duration,
            );
            let driver_thread = thread::spawn(move || {
                driver.start_server();
            });
            driver_thread.join().unwrap();
        },
        _ => {
            panic!("Only SOLO consensus be supported!");
        },
    }
}

#[test]
fn test_query_last_block() {
    println!("start get lastblock");
    let my_peer_id = [0 as u8];
    let context = zmq::Context::new();
    let chain_socket = context.socket(zmq::DEALER).unwrap();
    chain_socket.set_identity(&my_peer_id).unwrap();
    chain_socket.connect("tcp://127.0.0.1:9050").unwrap();
    let lastblock = query_last_block(&chain_socket);
    println!("receive lastblock: {:?}", lastblock);
}
