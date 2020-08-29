
use clap::{App, load_yaml, ArgMatches, Arg};
use std::str::FromStr;

use std::fs::File;
use std::io::prelude::*;
use std::path::Path;

use serde_derive::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub block_duration: Option<u64>,
    pub peers: Option<Vec<PeerConfig>>,
    pub index: Option<usize>,
    pub consensus: Option<String>,
    pub rpc_socket: Option<String>,
    pub engine_socket: Option<String>,
    pub chain_socket: Option<String>,
    pub p2p_socket: Option<String>,
    pub log_level: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PeerConfig {
    pub index: Option<usize>,
    pub p2p_socket: Option<String>,
}

pub fn read_config_file(file_path : &str) -> String {
    // Create a path to the desired file
    let path = Path::new(file_path);
    let display = path.display();
    // Open the path in read-only mode, returns `io::Result<File>`
    let mut file = match File::open(&path) {
        Err(why) => panic!("couldn't open {}: {}", display, why),
        Ok(file) => file,
    };
    // Read the file contents into a string, returns `io::Result<usize>`
    let mut ret = String::new();
    match file.read_to_string(&mut ret) {
        Err(why) => panic!("couldn't read {}: {}", display, why),
        Ok(_) => (),
        // Ok(_) => print!("{} contains:\n{}", display, ret),
    }
    // `file` goes out of scope, file gets closed
    ret
}

pub fn parse_config_string(config_string : &str) -> Config {
    let decoded_config: Config = toml::from_str(config_string).unwrap();
    decoded_config
}


#[test]
fn config_test() {
    let toml_str = read_config_file("src/bloom.conf");
    let decoded_config = parse_config_string(toml_str.as_str());
    println!("{:#?}", decoded_config);
}
