#![no_std]
#![no_main]

use esp32_hal::{
    clock::ClockControl, 
    peripherals::Peripherals, 
    prelude::*, 
    Delay,
    timer::TimerGroup, 
    Rng,
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

use protocol;


#[entry]
fn main() -> ! {
    init_heap();

    let peripherals = Peripherals::take();
    let system = peripherals.SYSTEM.split();

    let clocks = ClockControl::max(system.clock_control).freeze();
    let mut delay = Delay::new(&clocks);

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
        Rng::new(peripherals.RNG),
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
