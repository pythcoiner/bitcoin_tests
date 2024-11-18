use std::{collections::BTreeMap, usize};

use bitcoind::{
    bitcoincore_rpc::{json::AddressType, Client, RpcApi},
    BitcoinD,
};
use miniscript::bitcoin::{Sequence, Transaction};
use rand::Rng;
use utils::{generate_to_self, send_all_to_address, send_to_address};

use crate::utils::{bootstrap_bitcoind, create_wallet};

pub mod utils;

const MAX_INPUTS: usize = 5;
const TX_NUMBER: usize = 200;

type A = BTreeMap<Sequence, usize>;
type B = BTreeMap<(usize, usize), usize>;
type C = BTreeMap<i64, usize>;

fn init_maps() -> (A, B, C)
where
{
    let relative: BTreeMap<Sequence, usize> = BTreeMap::new();
    let relative_pos: BTreeMap<(usize, usize), usize> = BTreeMap::new();
    let absolute: BTreeMap<i64, usize> = BTreeMap::new();
    (relative, relative_pos, absolute)
}

fn spend(
    bitcoind: &BitcoinD,
    wallet: &Client,
    address_type: AddressType,
    rbf: bool,
    maps: &mut (A, B, C),
) {
    let relative = &mut maps.0;
    let relative_pos = &mut maps.1;
    let absolute = &mut maps.2;
    let mut rng = rand::thread_rng();

    for _ in 0..TX_NUMBER {
        let addr = wallet
            .get_new_address(None, Some(address_type))
            .unwrap()
            .assume_checked();
        let inp = rng.gen_range(1..(MAX_INPUTS + 1));
        let amount: f64 = 0.6 / inp as f64;
        for _ in 0..inp {
            send_to_address(&bitcoind.client, &addr, amount, rbf);
        }
        // generate 1 block
        generate_to_self(bitcoind, 1);

        // spend the change back to default wallet
        let addr = bitcoind
            .client
            .get_new_address(None, None)
            .unwrap()
            .assume_checked();
        let tx = send_to_address(wallet, &addr, 0.5, rbf);

        let block_height = wallet.get_block_count().unwrap();

        if tx.lock_time.to_consensus_u32() > 0 {
            let locktime = (tx.lock_time.to_consensus_u32() as i64)
                .checked_sub(block_height as i64)
                .unwrap();
            absolute
                .entry(locktime)
                .and_modify(|e| *e += 1)
                .or_insert(1);
        }

        let mut sequence_count = 0;
        let inp_number = inp as usize;
        for (index, inp) in tx.input.iter().enumerate() {
            if (inp.sequence != Sequence::ENABLE_RBF_NO_LOCKTIME && rbf)
            // when rbf disabled nSequence == ENABLE_LOCKTIME_NO_RBF
                || ((inp.sequence != Sequence::ENABLE_LOCKTIME_NO_RBF) && !rbf)
            {
                relative_pos
                    .entry((inp_number, index))
                    .and_modify(|e| *e += 1)
                    .or_insert(1);
                sequence_count += 1;
                relative
                    .entry(inp.sequence)
                    .and_modify(|e| *e += 1)
                    .or_insert(1);
            }
        }
        // we have maximum 1 nSequence value (1)
        assert!(sequence_count <= 1);
        // we never have both nSequence and nLockTime
        assert!(!((sequence_count > 0) && (tx.lock_time.to_consensus_u32() > 0)));

        // wipe the wallet
        let _tx = send_all_to_address(wallet, &addr);
        generate_to_self(bitcoind, 1);
    }
}

fn spend_taproot(bitcoind: &BitcoinD) {
    let taproot = create_wallet(bitcoind, "taproot");

    let mut maps = init_maps();
    spend(bitcoind, &taproot, AddressType::Bech32m, true, &mut maps);

    let relative = &mut maps.0;
    let relative_pos = &mut maps.1;
    let absolute = &mut maps.2;

    // nSequence is always 1
    assert!(relative.contains_key(&Sequence::from_height(1)));
    assert_eq!(relative.len(), 1);
    // sequence can be found at any input index
    // we keep conservative and target 80% of all combinations
    assert!(relative_pos.keys().len() >= (MAX_INPUTS * (MAX_INPUTS + 1) / 2) * 80 / 100);
    // more than 40% of tx have anti-sniping w/ nSequence
    assert!(*relative.get(&Sequence::from_height(1)).unwrap() > (TX_NUMBER * 40 / 100));

    // more than 40% of tx have anti-sniping w/ nLockTime
    let mut abs_count = 0;
    for v in absolute.values() {
        abs_count += v;
    }
    assert!(abs_count > (TX_NUMBER * 40 / 100));
    // 5-15% of nLockTime should be in a range of 0-100 blocks early than now
    assert!(*absolute.get(&0).unwrap() > (abs_count * 85 / 100));
    assert!(*absolute.get(&0).unwrap() < (abs_count * 95 / 100));
    assert!(*absolute.first_key_value().unwrap().0 > -101);
    assert!(*absolute.first_key_value().unwrap().0 < 0);
}

fn spend_taproot_no_rbf(bitcoind: &BitcoinD) {
    let wallet = create_wallet(bitcoind, "taproot_no_rbf");

    let mut maps = init_maps();
    spend(
        bitcoind,
        &wallet,
        AddressType::Bech32m,
        /*rbf=*/ false,
        &mut maps,
    );

    is_all_nlocktime(&mut maps);
}

fn spend_segwit(bitcoind: &BitcoinD) {
    let segwit = create_wallet(bitcoind, "segwit");

    let mut maps = init_maps();
    spend(bitcoind, &segwit, AddressType::Bech32, true, &mut maps);

    is_all_nlocktime(&mut maps);
}

fn is_all_nlocktime(maps: &mut (A, B, C)) {
    let relative = &mut maps.0;
    let absolute = &mut maps.2;

    // never nSequence w/ non taproot inputs
    assert_eq!(relative.len(), 0);

    // 100% of tx have anti-sniping w/ nLockTime
    let mut abs_count = 0;
    for v in absolute.values() {
        abs_count += v;
    }
    assert_eq!(abs_count, TX_NUMBER);
    // 5-15% of nLockTime should be in a range of 0-100 blocks early than now
    assert!(*absolute.get(&0).unwrap() > (abs_count * 85 / 100));
    assert!(*absolute.get(&0).unwrap() < (abs_count * 95 / 100));
    assert!(*absolute.first_key_value().unwrap().0 > -101);
    assert!(*absolute.first_key_value().unwrap().0 < 0);
}

fn spend_taproot_segwit(bitcoind: &BitcoinD) {
    let wallet = create_wallet(bitcoind, "wallet");

    for _ in 0..TX_NUMBER {
        // create a segwit input
        let addr = wallet
            .get_new_address(None, Some(AddressType::Bech32))
            .unwrap()
            .assume_checked();
        send_to_address(&bitcoind.client, &addr, 0.3, true);
        // create a taproot input
        let addr = wallet
            .get_new_address(None, Some(AddressType::Bech32m))
            .unwrap()
            .assume_checked();
        send_to_address(&bitcoind.client, &addr, 0.3, true);
        // generate 1 block
        generate_to_self(bitcoind, 1);

        // spend coins
        let addr = bitcoind
            .client
            .get_new_address(None, None)
            .unwrap()
            .assume_checked();
        let tx = send_to_address(&wallet, &addr, 0.5, true);
        let block_height = wallet.get_block_count().unwrap();

        is_nlocktime_only(block_height, &tx);

        // wipe the wallet
        let _tx = send_all_to_address(&wallet, &addr);
        generate_to_self(bitcoind, 1);
    }
}

fn spend_unconfirmed(bitcoind: &BitcoinD) {
    let wallet = create_wallet(bitcoind, "unconfirmed");

    for _ in 0..TX_NUMBER {
        // create 2 inputs
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
        // confirm both
        generate_to_self(bitcoind, 1);
        // create an unconfirmed send-to-self (unconfirmed from external inputs are not used by
        // "sendtoadress")
        let addr = wallet
            .get_new_address(None, Some(AddressType::Bech32m))
            .unwrap()
            .assume_checked();
        send_to_address(&wallet, &addr, 0.2, true);

        // spend coins
        let addr = bitcoind
            .client
            .get_new_address(None, None)
            .unwrap()
            .assume_checked();
        let tx = send_to_address(&wallet, &addr, 0.5, true);
        let block_height = wallet.get_block_count().unwrap();

        is_nlocktime_only(block_height, &tx);

        // wipe the wallet
        let _tx = send_all_to_address(&wallet, &addr);
        generate_to_self(bitcoind, 1);
    }
}

fn is_nlocktime_only(block_height: u64, tx: &Transaction) {
    for inp in &tx.input {
        // never anti-sniping with nSequence
        assert_eq!(inp.sequence, Sequence::ENABLE_RBF_NO_LOCKTIME);
    }
    // always anti-sniping with nLocktime and between 0-100 blocks back
    assert!(tx.lock_time.to_consensus_u32() > 0);
    let locktime = (tx.lock_time.to_consensus_u32() as i64)
        .checked_sub(block_height as i64)
        .unwrap();
    assert!(locktime < 1);
    assert!(locktime > -101);
}

fn main() {
    let bitcoind = bootstrap_bitcoind();

    // test with only taproot inputs
    spend_taproot(&bitcoind);
    println!("spend_taproot passed!");

    // test with only taproot inputs no rbf
    spend_taproot_no_rbf(&bitcoind);
    println!("spend_taproot_no_rbf passed!");

    // test with only segwit inputs
    spend_segwit(&bitcoind);
    println!("spend_taproot passed!");

    // spend with mixed inputs
    spend_taproot_segwit(&bitcoind);
    println!("spend_taproot_segwit passed!");

    // spend with unconfirmed input
    spend_unconfirmed(&bitcoind);
    println!("spend_unconfirmed passed!");
}
