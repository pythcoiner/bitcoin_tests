use std::{env, path::PathBuf};

use bitcoind::{
    bitcoincore_rpc::{jsonrpc::serde_json::Value, Client, RpcApi},
    BitcoinD, P2P,
};
use miniscript::bitcoin::{consensus::encode, Address, Amount, Transaction};

pub fn bootstrap_bitcoind() -> BitcoinD {
    let mut cwd: PathBuf = env::current_dir().expect("Failed to get current directory");
    cwd.push("src");

    let mut bitcoind_path = cwd.clone();
    bitcoind_path.push("bin");
    bitcoind_path.push("bitcoind_pr24128");

    let mut conf = bitcoind::Conf::default();
    conf.p2p = P2P::Yes;
    let bitcoind = BitcoinD::with_conf(bitcoind_path, &conf).unwrap();

    // mine 200 blocks
    let node_address = bitcoind.client.call::<Value>("getnewaddress", &[]).unwrap();
    bitcoind
        .client
        .call::<Value>("generatetoaddress", &[400.into(), node_address])
        .unwrap();

    bitcoind
}

pub fn create_wallet(bitcoind: &BitcoinD, name: &str) -> Client {
    bitcoind
        .client
        .call::<Value>("createwallet", &[Value::String(name.into())])
        .unwrap();
    let url = bitcoind.rpc_url_with_wallet(name);
    let mut cookie_path = bitcoind.workdir();
    cookie_path.push("regtest");
    cookie_path.push(".cookie");

    Client::new(
        &url,
        bitcoind::bitcoincore_rpc::Auth::CookieFile(cookie_path),
    )
    .unwrap()
}

pub fn send_to_address(client: &Client, addr: &Address, amount: f64, rbf: bool) -> Transaction {
    let amount = (amount * 10000.0).ceil() / 10000.0;
    let amount = Amount::from_btc(amount).unwrap();
    let txid = client.send_to_address(addr, amount, None, None, None, Some(rbf), None, None);
    match txid {
        Ok(txid) => client.get_raw_transaction(&txid, None).unwrap(),
        Err(e) => {
            println!("{e}");
            let balance = client.get_balance(None, None).unwrap();
            println!("balance: {}", balance);
            panic!();
        }
    }
}

pub fn send_all_to_address(client: &Client, addr: &Address) -> Option<Transaction> {
    client
        .call::<Value>(
            "sendall",
            &[Value::Array(vec![Value::String(addr.to_string())])],
        )
        .ok()
        .map(|r| {
            let map = r.as_object().unwrap();
            let txid = map.get("txid").unwrap().clone();
            let tx = client.call::<Value>("getrawtransaction", &[txid]).unwrap();
            encode::deserialize_hex(tx.as_str().unwrap()).unwrap()
        })
}

pub fn generate_to_self(bitcoind: &BitcoinD, blocks: u32) {
    let node_address = bitcoind.client.call::<Value>("getnewaddress", &[]).unwrap();
    bitcoind
        .client
        .call::<Value>("generatetoaddress", &[blocks.into(), node_address])
        .unwrap();
}
