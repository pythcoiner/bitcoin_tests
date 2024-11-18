use bitcoind::bitcoincore_rpc::{json::AddressType, RpcApi};
use utils::{generate_to_self, send_all_to_address, send_to_address};

use crate::utils::{bootstrap_bitcoind, create_wallet};

pub mod utils;

fn main() {
    let bitcoind = bootstrap_bitcoind();
    let wallet = create_wallet(&bitcoind, "taproot");

    for _ in 0..100 {
        // create 2 inputs to wallet "taproot"
        let addr = wallet
            .get_new_address(None, Some(AddressType::Bech32m))
            .unwrap()
            .assume_checked();
        send_to_address(&bitcoind.client, &addr, 0.3, true);
        let addr = wallet
            .get_new_address(None, Some(AddressType::Bech32m))
            .unwrap()
            .assume_checked();
        send_to_address(&bitcoind.client, &addr, 0.3, true);

        // spend coins to wallet "default"
        let addr = bitcoind
            .client
            .get_new_address(None, None)
            .unwrap()
            .assume_checked();

        // wipe the wallet
        let _tx = send_all_to_address(&wallet, &addr);
        generate_to_self(&bitcoind, 1);
    }
}
