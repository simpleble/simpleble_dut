#![no_std]
#![no_main]

use defmt_rtt as _; // global logger
use embassy_nrf as _; // time driver
use panic_probe as _;

use defmt::{info, *};
use embassy_executor::Spawner;
use embassy_time::{Duration, Timer, *};
use embassy_nrf::interrupt::Priority;
use nrf_softdevice::{Softdevice};

use embassy_nrf::gpio::{Level, Output, OutputDrive};
mod itc;
mod ble;

use ble::{softdevice_config, softdevice_task, comms_ble_task, BleController, BleEventReceiver, BleEvent};

#[embassy_executor::task]
async fn background_task() -> ! {
    loop {

        info!("background task");
        defmt::flush();
        block_for(Duration::from_millis(1000));
         Timer::after(Duration::from_millis(100)).await;
        // info!("background task done");
    }
}

#[embassy_executor::task]
async fn led_control_task(mut led_r: Output<'static>, mut led_g: Output<'static>, mut led_b: Output<'static>, ble_event_receiver: BleEventReceiver) -> ! {
    led_r.set_high();
    led_g.set_high();
    led_b.set_high();
    loop {
        let event = ble_event_receiver.receive().await;
        match event {
            BleEvent::Idle => {
                info!("Idle");
            }
            BleEvent::Advertising => {
                info!("Advertising");
                led_b.set_low();
            }
            BleEvent::Connected => {
                info!("Connected");
                led_b.set_high();
                led_r.set_low();
            }
            BleEvent::Disconnected => {
                info!("Disconnected");
                led_r.set_high();
                led_g.set_high();
                led_b.set_high();
            }
        }
    }
}

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    info!("Hello World!");

    let mut config = embassy_nrf::config::Config::default();
    config.gpiote_interrupt_priority = Priority::P2;
    config.time_interrupt_priority = Priority::P2;
    let p = embassy_nrf::init(config);

    let led_r = Output::new(p.P0_30, Level::Low, OutputDrive::Standard);
    let led_g = Output::new(p.P0_29, Level::Low, OutputDrive::Standard);
    let led_b = Output::new(p.P0_31, Level::Low, OutputDrive::Standard);

    let config = softdevice_config();
    let sd = Softdevice::enable(&config);
    let ble_controller = BleController::new(sd);
    let ble_event_receiver = ble_controller.get_ble_event_receiver();

    unwrap!(spawner.spawn(softdevice_task(sd)));
    unwrap!(spawner.spawn(comms_ble_task(ble_controller, sd)));
    //unwrap!(spawner.spawn(dut_task(ble_controller)));
    unwrap!(spawner.spawn(led_control_task(led_r, led_g, led_b, ble_event_receiver)));
    //unwrap!(spawner.spawn(background_task()));


    loop {
        Timer::after(Duration::from_secs(1)).await;
    }
}
