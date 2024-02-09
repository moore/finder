#![no_std]
#![no_main]

use esp32_hal::{
    clock::ClockControl, peripherals::Peripherals, prelude::*, timer::TimerGroup, Rng,
};

use esp_backtrace as _;
use esp_println::println;

use esp_wifi::esp_now::{PeerInfo, BROADCAST_ADDRESS};
use esp_wifi::{current_millis, initialize, EspWifiInitFor};

use core::mem::MaybeUninit;
extern crate alloc;

#[global_allocator]
static ALLOCATOR: esp_alloc::EspHeap = esp_alloc::EspHeap::empty();

fn init_heap() {
    const HEAP_SIZE: usize = 32 * 1024;
    static mut HEAP: MaybeUninit<[u8; HEAP_SIZE]> = MaybeUninit::uninit();

    unsafe {
        ALLOCATOR.init(HEAP.as_mut_ptr() as *mut u8, HEAP_SIZE);
    }
}

use critical_section;

use protocol::{
    wire,
    crypto::rust::{
        RsaPrivateKey,
        RsaPublicKey,
        RustCrypto,
    },
    crypto::KeyPair,
    heap_type::StaticAllocation,
    storage::mem_io::MemIO,
    Client,
    ClientChannels,
};


use rsa::pkcs1::DecodeRsaPrivateKey;

const MEGA_BYTE: usize = 1024 * 10; //* 1024;
const SLAB_SIZE: usize = 1024;
const MAX_CHANNELS: usize = 4;
const MAX_NODES: usize = 2;

#[entry]
fn main() -> ! {
    init_heap();

    //////// init a protocol client //////
    let peripherals = Peripherals::take();

    let mut rng = Rng::new(peripherals.RNG);

    
    let mut seed = [0; 128];

    critical_section::with(|_cs| {
        // BUG: the docs for Rng say that I either
        // need to have the radio on or use
        // these to make sure I am really getting 
        // random numbers but this seems to be a idf 
        // thing. Do I really need to do it and if I do 
        // how?
        //???::bootloader_random_enable();
        rng.read(&mut seed).unwrap();
        //???::bootloader_random_disable();
    });

    let mut crypto = RustCrypto::new(&seed).unwrap();
    let key_pair = get_test_keys();
    static BUFFER: StaticAllocation<[u8; MEGA_BYTE]> = StaticAllocation::wrap([0u8; MEGA_BYTE]);
    let data = BUFFER.take_mut().unwrap();
    let io: MemIO<'_, SLAB_SIZE> = MemIO::new(data).unwrap();

    static CHANNELS_CONST: StaticAllocation<
        ClientChannels<MAX_CHANNELS, MAX_NODES, MemIO<'_, SLAB_SIZE>, RustCrypto>,
    > = StaticAllocation::wrap(ClientChannels::new());

    let channels = CHANNELS_CONST.take_mut().unwrap();

    let mut client: Client<'_, '_, MAX_CHANNELS, MAX_NODES, MemIO<'_, SLAB_SIZE>, RustCrypto> =
        Client::new(key_pair, &mut crypto, channels);

    let name_str = "Test Chat";
    let channel_id = client.init_chat(name_str, io).unwrap();
    let nodes = client.list_nodes(&channel_id).unwrap();
    //assert_eq!(nodes.len(), 1);
    println!("got {:?} nodes", nodes.len());
    
    //// end protocol /////

    let system = peripherals.SYSTEM.split();

    let clocks = ClockControl::max(system.clock_control).freeze();

    // setup logger
    // To change the log_level change the env section in .cargo/config.toml
    // or remove it and set ESP_LOGLEVEL manually before running cargo run
    // this requires a clean rebuild because of https://github.com/rust-lang/cargo/issues/10358
    esp_println::logger::init_logger_from_env();
    log::info!("Logger is setup");
    println!("Hello world!");
    let timer = TimerGroup::new(peripherals.TIMG1, &clocks).timer0;
    let init = initialize(
        EspWifiInitFor::Wifi,
        timer,
        rng,
        system.radio_clock_control,
        &clocks,
    )
    .unwrap();

    let wifi = peripherals.WIFI;
    let mut esp_now = esp_wifi::esp_now::EspNow::new(&init, wifi).unwrap();

    println!("esp-now version {}", esp_now.get_version().unwrap());

    let mut next_send_time = current_millis() + 5 * 1000;
    loop {
        let r = esp_now.receive();
        if let Some(r) = r {
            println!("Received {:?}", r);

            if r.info.dst_address == BROADCAST_ADDRESS {
                if !esp_now.peer_exists(&r.info.src_address) {
                    esp_now
                        .add_peer(PeerInfo {
                            peer_address: r.info.src_address,
                            lmk: None,
                            channel: None,
                            encrypt: false,
                        })
                        .unwrap();
                }
                let status = esp_now
                    .send(&r.info.src_address, b"Hello Peer")
                    .unwrap()
                    .wait();
                println!("Send hello to peer status: {:?}", status);
            }
        }

        if current_millis() >= next_send_time {
            next_send_time = current_millis() + 5 * 1000;
            println!("Send");
            let status = esp_now
                .send(&BROADCAST_ADDRESS, b"0123456789")
                .unwrap()
                .wait();
            println!("Send broadcast status: {:?}", status)
        }
    }

    /*
    loop {
        println!("Loop...");
        delay.delay_ms(500u32);
    }
    */
}


const PRIVATE_KEY: &str = "-----BEGIN RSA PRIVATE KEY-----
MIIEowIBAAKCAQEAt+15Q+QlwFThI33dHA4qCFSmX35CsJBOMKAAH8TzhoTl5TL+
sv9861tvxlMgY181VDyvZWcUYHIqToFZKEEeVox4t3VtrTciJlCpfWGjDXsWvLGo
V4ExSkTXBF1P4oe+JRc5dz3T7Wviwa7QN+Mt9IGsaL9Qtq4XpQY03UoKLIbgnxjW
r0kkWrRoF5vDDaxBC6UqkONAE6z+JbhBF1e9VFd/+1NWzj3Go8xFTVcvfykWBy7l
djqSdJmMK3WV7R7gikYtdOMRug0Bt7UvFM5JMpRtf7FSEG7khalyppqtBiSW3zzu
lo+Hulki1b8jr10W6KUS2rzKc+A91yqy6lEeNwIDAQABAoIBADptiu9BQ6jUjeyr
aBkoervIwE1nm6HhRaV2vnNZKo9aGnnz+Cs+tB1EH77d21UWAqfu2z0YQMXenofv
2TXLcerGlvaYrC2xbPzE9QKqiJSYvIFW4oZhuRnBwphVWDI7MvEvbobtsiwi8Jbc
hLKsTYX1x6JC3E4cAdDfpt2BTrgT2s/eOfTHhMVpErGAg/0Qljy/Vg7hUiIgyTED
/Y5mfe/RJHXsz4ekkg5EtdwHkVr35zfe3O9wWf2HAxGVWFrAX+CKD62CK/tDrawu
g/sSPi5wSTqUFtcNYZCeQUwkz4sWS4jrwkl2nKQ4G+lrfRPXSZ7tDBGJXtR2KZf0
WY00WYkCgYEA02T0wCnGmlFC5Jl+AisxHWVaqoUD+hVKPnmjpLlua+8Xx5Tz7SEi
KDyJy9O2Vz7366VyIW+4c+jpltoPqtHnEjIava1nqN8GtTyrFl5gQKuSOSBsnBnL
63rY4I5NvdDiwqDo9tHUDoYDmeNkSpaj/1i84EmxtbsPPYQib6Ghnk0CgYEA3rzT
yC5CvQf7Z6V5gOx1au2ULtaAOLuLDXZIAMM7z42qmQS8SEkmEw/Vr6gRGClBh6QT
ZYxKTOegwKUYVBeheV1Y5Nvu1Jd/IwfuEibBVrZR1AfyhB/HIQ4QLmo33LP/rcPj
dwi7pPHsi9FMDJ1cRc52lAu1FljeAp9R+VBcGJMCgYB/JUe4lOfpRVsQl+mccFIY
Ni/0RBECR+/h59OvbgCmVqZc2pBkXftnbBINUIdprmv7hgVBayrsPHjSzNGDksCC
xzQiRbwFbC9irtzQlW8bNpa6WXA566IlPjxXw/+qXYsmORYl7kq3eY+M7aIS4sw8
9yiTVn/WqG4gN+tmbTcCOQKBgQCKTExjGvYtUOt0q3YJ6sftIJ7FhkIO98ObFDoY
3yAf+yJV6G7PozuU0lwnuP8ENXmOsv2oK7dmkNtrQhcc/58vMBql3zknnvk90wqr
Eo0xPfsI3/Zguyp1B7pcV29gBhNW3S47Fp0MCXqKReYmXv6QCWXu/mXt/je7ARlw
58iHKQKBgF5Gbm9Vm9VslX1ip9Et6Sev6u56bopYqLFKhy4mVjgm7wMRlv5k+oTK
v7I4OZ5SdijZfALO8oaYd9gjSFhxUq9bA2YXxzl06JfLPNTG93QORySJKNnEQ91V
BHm4I4zAJFmYCL/mBGIjhDI5q7YM7aHpQsDIIrx84vFbJqJfrJem
-----END RSA PRIVATE KEY-----
";

pub fn get_test_keys() -> KeyPair<RsaPrivateKey, RsaPublicKey> {
    let private = RsaPrivateKey::from_pkcs1_pem(PRIVATE_KEY).expect("error reading key");
    let public = private.to_public_key();
    KeyPair { private, public }
}


