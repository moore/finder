#![no_std]
#![no_main]

use esp32_hal::{
    clock::ClockControl, peripherals::Peripherals, prelude::*, timer::TimerGroup, Rng,
};

use esp_backtrace as _;
use esp_println::println;

use esp_wifi::esp_now::{EspNow, PeerInfo, BROADCAST_ADDRESS};
use esp_wifi::{current_millis, initialize, EspWifiInitFor};

use core::mem::{size_of, size_of_val, MaybeUninit};
extern crate alloc;

#[global_allocator]
static ALLOCATOR: esp_alloc::EspHeap = esp_alloc::EspHeap::empty();

const HEAP_SIZE: usize = 64 * 1024;
fn init_heap() {
    static mut HEAP: MaybeUninit<[u8; HEAP_SIZE]> = MaybeUninit::uninit();

    unsafe {
        ALLOCATOR.init(HEAP.as_mut_ptr() as *mut u8, HEAP_SIZE);
    }
}

use critical_section;
use heapless::Vec;
use postcard::{from_bytes, to_slice};

use protocol::{
    wire::{
        NetworkProtocol,
        ChannelInfo,
        WireWriter,
        WireReader,
        WireError,
    },
    crypto::{
        Crypto,
        ChannelId,
        rust::{
            RsaPrivateKey,
            RsaPublicKey,
            RustCrypto,
        }
    },
    crypto::KeyPair,
    heap_type::StaticAllocation,
    storage::{
        IO,
        mem_io::MemIO,
    },
    Client,
    ClientChannels,
};


use rsa::pkcs1::{DecodeRsaPrivateKey, EncodeRsaPublicKey};

const MEGA_BYTE: usize = 1024 * 10; //* 1024;
const SLAB_SIZE: usize = 1024;
const MAX_CHANNELS: usize = 4;
const MAX_NODES: usize = 2;
const ESP_NOW_MTU: u16 = 250;
const MAX_RESPONSE: usize = 1024 * 10;
const REPAIR_COUNT: u32 = 3;
const WIFI_HEAP: usize = 65536;

const MESSAGE_MAX: usize = size_of::<NetworkProtocol<MAX_CHANNELS, MAX_NODES, MAX_RESPONSE>>();

static MESSAGE_BUFFER: StaticAllocation<[u8; MESSAGE_MAX]> = StaticAllocation::wrap([0u8 ; MESSAGE_MAX]);
static MEMIO_BUFFER: StaticAllocation<[u8; MEGA_BYTE]> = StaticAllocation::wrap([0u8; MEGA_BYTE]);
static CHANNELS_CONST: StaticAllocation<
        ClientChannels<MAX_CHANNELS, MAX_NODES, MemIO<'_, SLAB_SIZE>, RustCrypto>,
    > = StaticAllocation::wrap(ClientChannels::new());

#[entry]
fn main() -> ! {
    init_heap();

    // setup logger
    // To change the log_level change the env section in .cargo/config.toml
    // or remove it and set ESP_LOGLEVEL manually before running cargo run
    // this requires a clean rebuild because of https://github.com/rust-lang/cargo/issues/10358
    esp_println::logger::init_logger_from_env();
    log::info!("Logger is setup");

    log::info!("\n Heap size \t{}\n MESSAGE_BUFFER {}\n MEMIO_BUFFER \t{}\n CHANNELS_CONST {}\n WiFi Heap \t{}\nTotal \t\t{}", 
    HEAP_SIZE, 
    size_of_val(&MESSAGE_BUFFER),
    size_of_val(&MEMIO_BUFFER),
    size_of_val(&CHANNELS_CONST),
    WIFI_HEAP,
    HEAP_SIZE 
    + size_of_val(&MESSAGE_BUFFER)
    + size_of_val(&MEMIO_BUFFER)
    + size_of_val(&CHANNELS_CONST)
    + WIFI_HEAP,
    );
    
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
        // I do see in the boot logs that this was turned 
        // on for a bit. Was that enough?
        //???::bootloader_random_enable();
        rng.read(&mut seed).unwrap();
        //???::bootloader_random_disable();
    });

    let mut crypto = RustCrypto::new(&seed).unwrap();
    let key_pair = get_test_keys();
    let data = MEMIO_BUFFER.take_mut().unwrap();
    let io: MemIO<'_, SLAB_SIZE> = MemIO::new(data).unwrap();

    
    let channels = CHANNELS_CONST.take_mut().unwrap();

    let mut client: Client<'_, '_, MAX_CHANNELS, MAX_NODES, MemIO<'_, SLAB_SIZE>, RustCrypto> =
        Client::new(key_pair, &mut crypto, channels);

    let name_str = "Test Chat";
    let channel_id = client.init_chat(name_str, io).unwrap();
    let nodes = client.list_nodes(&channel_id).unwrap();
    //assert_eq!(nodes.len(), 1);
    log::info!("got {} nodes", nodes.len());
    


    //// end protocol /////

    let system = peripherals.SYSTEM.split();

    let clocks = ClockControl::max(system.clock_control).freeze();
    log::info!("got clock");


    let timer = TimerGroup::new(peripherals.TIMG1, &clocks).timer0;
    let init = initialize(
        EspWifiInitFor::Wifi,
        timer,
        rng,
        system.radio_clock_control,
        &clocks,
    )
    .unwrap();

    log::info!("got timer");

    let wifi = peripherals.WIFI;
    let mut esp_now = esp_wifi::esp_now::EspNow::new(&init, wifi).unwrap();

    log::info!("esp-now version {} size {}", 
        esp_now.get_version().unwrap(),
        size_of_val(&esp_now));

    network_loop(&mut esp_now, channel_id, &mut client);

}


pub fn network_loop<
const MAX_CHANNELS: usize, 
const MAX_NODES: usize,
I: IO,
C: Crypto,
>(esp_now: &mut EspNow, channel_id: ChannelId, client: &mut Client<MAX_CHANNELS, MAX_NODES, I, C>) -> !{
    let  message_buffer = MESSAGE_BUFFER.take_mut()
    .expect("could not take message buffer");

    let mut message_number: u16 = 0;
    let mut next_send_time = current_millis() + 5 * 1000;
    let mut maybe_receiver = None;
    let mut last_completed = None;
    loop {
        let r = esp_now.receive();
        if let Some(r) = r {
            let data: &[u8] = r.get_data();

            let received_message_number = match WireReader::check_packet(data) {
                Ok(number) => number,
                Err(e) => {
                    log::info!("check packet failed {:?}", e);
                    continue;
                }
                
            };

            if let Some(finished) = last_completed {
                if received_message_number == finished {
                    continue;
                }
            }
            if maybe_receiver.is_none() {
                let Ok(receiver) = WireReader::new(data, ESP_NOW_MTU) else {
                    log::info!("Could not construct wire reader");
                    continue;
                };
                maybe_receiver = Some(receiver);
            }

            let Some(ref mut receiver) = maybe_receiver else {
                unreachable!("maybe_receiver empty after being set!!!")
            };
           
            log::info!("receiver block {}, message len {} data len {}", receiver.message_number, receiver.transfer_length, r.get_data().len());

            let result = match receiver.accept_packet(&data) {
                Ok(r) => r,
                Err(WireError::WrongBlock(_found)) => {
                    let Ok(mut receiver) = WireReader::new(&data, ESP_NOW_MTU) else {
                        log::info!("Could not construct wire reader");
                        continue;
                    };
                    let result = match receiver.accept_packet(&data) {
                        Ok(r) => r,
                        Err(e) => {
                            log::info!("could not accept packet because {:?}", e);
                            continue;
                        }
                    };
                    maybe_receiver = Some(receiver);
                    result
                },
                Err(e) => {
                    log::info!("could not accept packet because {:?}", e);
                    continue;
                }
            };

            if let Some(value) = result {
                let command: NetworkProtocol<MAX_CHANNELS, MAX_NODES, MAX_RESPONSE> = from_bytes(&value)
                    .expect("could not parse message");
                last_completed = Some(received_message_number);
                log::info!("Got a result {:?}", command);
            }

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
                /*
                let status = esp_now
                    .send(&r.info.src_address, b"Hello Peer")
                    .unwrap()
                    .wait();
                log::info!("Send hello to peer status: {:?}", status);
                */
            }
        }

        if current_millis() >= next_send_time {
            next_send_time = current_millis() + 5 * 1000;
            log::info!("Send");
            let hello: NetworkProtocol<MAX_CHANNELS, MAX_NODES, MAX_RESPONSE> 
                = make_hello(0, channel_id, &client);

            let written = to_slice(&hello, message_buffer)
                .expect("could not write to buffer");

            let mut writer = WireWriter::new(message_number, ESP_NOW_MTU, &written, REPAIR_COUNT);
            message_number += 1;

            for _ in 0..writer.packet_count() {
                let mut buffer = [0u8 ; 250];

                let len = writer.next(&mut buffer)
                    .expect("could not write packet!");

                let data = &buffer[0..len];

                let status = esp_now
                    .send(&BROADCAST_ADDRESS, data)
                    .unwrap()
                    .wait();
                log::info!("Send broadcast status: {:?}", status)
            }
        }
    }
}

fn make_hello<
    const MAX_CHANNELS: usize, 
    const MAX_NODES: usize,
    const MAX_RESPONSE: usize,
    I: IO,
    P: Crypto,
    >(peer_count: u8, channel_id: ChannelId, client: &Client<MAX_CHANNELS, MAX_NODES, I, P>) -> NetworkProtocol<MAX_CHANNELS, MAX_NODES, MAX_RESPONSE> {
    let message_count = client.message_count(&channel_id)
        .expect("could not get message count");

    let info = ChannelInfo {
        channel_id: channel_id.clone(),
        message_count,
    };
    let mut channel_info = Vec::new();
    channel_info.push(info).expect("too many channels");

    let node_id = client.get_node_id();
    
     NetworkProtocol::Hello {
        pub_key_id: node_id,
        peer_count,
        channel_info,
     }
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


