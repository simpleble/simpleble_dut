use core::cell::RefCell;
use core::mem;
use core::mem::MaybeUninit;
use defmt::*;
use embassy_futures::select::{select3, Either3};
use embassy_time::{Duration, Timer, *};
use nrf_softdevice::ble::advertisement_builder::{
    Flag, LegacyAdvertisementBuilder, LegacyAdvertisementPayload, ServiceList, ServiceUuid16,
};
use nrf_softdevice::ble::{gatt_server, peripheral, Connection};
use nrf_softdevice::{raw, Softdevice};

use crate::itc::*;

pub enum BleEvent {
    Idle,
    Advertising,
    Connected,
    Disconnected,
}

#[derive(Clone, Copy)]
pub struct StreamConfig {
    prefix: u8,
    interval_ms: u16,
}

define_channel!(BleEvent, BleEvent, 4);
define_channel!(NotifyStreamConfig, StreamConfig, 4);
define_channel!(IndicateStreamConfig, StreamConfig, 4);

// #[nrf_softdevice::gatt_service(uuid = "180f")]
// pub struct BatteryService {
//     #[characteristic(uuid = "2a19", read, notify)]
//     battery_level: u8,
// }

#[nrf_softdevice::gatt_service(uuid = "ee5555ee-0000-1111-2222-4444ff5555ff")]
pub struct DutGattService {
    #[characteristic(uuid = "ee5555ee-0001-1111-2222-4444ff5555ff", read)]
    status: u64,
    #[characteristic(uuid = "ee5555ee-0002-1111-2222-4444ff5555ff", write, write_without_response)]
    control: u64,
    #[characteristic(uuid = "ee5555ee-0003-1111-2222-4444ff5555ff", notify)]
    notify_stream: u16,
    #[characteristic(uuid = "ee5555ee-0004-1111-2222-4444ff5555ff", indicate)]
    indicate_stream: u16,
}

#[nrf_softdevice::gatt_server]
pub struct GattServer {
    dut: DutGattService,
}

impl GattServer {

}

pub fn softdevice_config() -> nrf_softdevice::Config {
    nrf_softdevice::Config {
        clock: Some(raw::nrf_clock_lf_cfg_t {
            source: raw::NRF_CLOCK_LF_SRC_RC as u8,
            rc_ctiv: 16,
            rc_temp_ctiv: 2,
            accuracy: raw::NRF_CLOCK_LF_ACCURACY_500_PPM as u8,
        }),
        conn_gap: Some(raw::ble_gap_conn_cfg_t {
            conn_count: 1,
            event_length: 24,
        }),
        conn_gatt: Some(raw::ble_gatt_conn_cfg_t { att_mtu: 512 }),
        gatts_attr_tab_size: Some(raw::ble_gatts_cfg_attr_tab_size_t {
            attr_tab_size: raw::BLE_GATTS_ATTR_TAB_SIZE_DEFAULT,
        }),
        gap_role_count: Some(raw::ble_gap_cfg_role_count_t {
            adv_set_count: 1,
            periph_role_count: 1,
            central_role_count: 0,
            central_sec_count: 0,
            _bitfield_1: raw::ble_gap_cfg_role_count_t::new_bitfield_1(0),
        }),
        gap_device_name: Some(raw::ble_gap_cfg_device_name_t {
            p_value: b"SimpleBLE DUT" as *const u8 as _,
            current_len: 14,
            max_len: 14,
            write_perm: unsafe { mem::zeroed() },
            _bitfield_1: raw::ble_gap_cfg_device_name_t::new_bitfield_1(raw::BLE_GATTS_VLOC_STACK as u8),
        }),
        ..Default::default()
    }
}

pub struct BleController {
    server: GattServer,
    ble_event_channel: BleEventChannel,
    ble_event_sender: BleEventSender,
    notify_stream_active: RefCell<bool>,
    notify_stream_channel: NotifyStreamConfigChannel,
    notify_stream_sender: NotifyStreamConfigSender,
    indicate_stream_active: RefCell<bool>,
    indicate_stream_channel: IndicateStreamConfigChannel,
    indicate_stream_sender: IndicateStreamConfigSender,
}

impl BleController {
    #[allow(static_mut_refs)] // TODO: Research the rusty way to do this
    pub fn new(sd: &mut Softdevice) -> &'static mut Self {
        static mut BLE_SERVER: MaybeUninit<BleController> = MaybeUninit::uninit();

        // Safety: This is only called once during initialization,
        // before any concurrent access is possible
        unsafe {
            let ble_event_channel = BleEventChannel::new();
            let ble_event_sender = ble_event_channel.sender();

            let notify_stream_channel = NotifyStreamConfigChannel::new();
            let notify_stream_sender = notify_stream_channel.sender();

            let indicate_stream_channel = IndicateStreamConfigChannel::new();
            let indicate_stream_sender = indicate_stream_channel.sender();

            let p = BLE_SERVER.as_mut_ptr();
            p.write(BleController {
                server: unwrap!(GattServer::new(sd)),
                ble_event_channel: ble_event_channel,
                ble_event_sender: ble_event_sender,
                notify_stream_active: RefCell::new(false),
                notify_stream_channel: notify_stream_channel,
                notify_stream_sender: notify_stream_sender,
                indicate_stream_active: RefCell::new(false),
                indicate_stream_channel: indicate_stream_channel,
                indicate_stream_sender: indicate_stream_sender,
            });
            &mut *p
        }
    }

    pub fn get_ble_event_receiver(&self) -> BleEventReceiver {
        self.ble_event_channel.receiver()
    }

    fn handle_control_write(&self, value: u64) {
        // Parse first StreamConfig (bottom 3 bytes)
        let notify_config = StreamConfig {
            prefix: ((value >> 16) & 0xFF) as u8,
            interval_ms: (value & 0xFFFF) as u16,
        };

        // Parse second StreamConfig (next 3 bytes)
        let indicate_config = StreamConfig {
            prefix: ((value >> 40) & 0xFF) as u8,
            interval_ms: ((value >> 24) & 0xFFFF) as u16,
        };

        // Send configurations to respective streams
        let _ = self.notify_stream_sender.try_send(notify_config);
        let _ = self.indicate_stream_sender.try_send(indicate_config);
    }

    async fn handle_notify_stream(&self, server: &GattServer, connection: &Connection, cmd_receiver: NotifyStreamConfigReceiver) {
        let mut config = StreamConfig {
            prefix: 0x28,     // Default prefix
            interval_ms: 100, // Default interval
        };
        let mut counter: u8 = 0;

        loop {
            if !*self.notify_stream_active.borrow() {
                Timer::after(Duration::from_millis(250)).await;
                continue;
            }

            // Check for configuration updates
            match cmd_receiver.try_receive() {
                Ok(new_config) => {
                    config = new_config;
                }
                Err(_) => {} // No new command
            }

            // Construct the u16 value
            let value = ((config.prefix as u16) << 8) | (counter as u16);

            // Send notification
            unwrap!(server.dut.notify_stream_notify(connection, &value));
            counter = counter.wrapping_add(1);

            // Wait for the configured interval
            Timer::after(Duration::from_millis(config.interval_ms as u64 + 5)).await;
        }
    }

    async fn handle_indicate_stream(&self, server: &GattServer, connection: &Connection, cmd_receiver: IndicateStreamConfigReceiver) {
        let mut config = StreamConfig {
            prefix: 0x28,     // Default prefix
            interval_ms: 1000, // Default interval
        };
        let mut counter: u8 = 0;

        loop {
            if !*self.indicate_stream_active.borrow() {
                Timer::after(Duration::from_millis(250)).await;
                continue;
            }

            // Check for configuration updates
            match cmd_receiver.try_receive() {
                Ok(new_config) => {
                    config = new_config;
                }
                Err(_) => {} // No new command
            }

            // Construct the u16 value
            let value = ((config.prefix as u16) << 8) | (counter as u16);

            // Send indication
            match server.dut.indicate_stream_indicate(connection, &value) {
                Ok(_) => {}
                Err(e) => {
                    error!("indicate stream error: {:?}", e);
                }
            }
            counter = counter.wrapping_add(1);

            // Wait for the configured interval
            Timer::after(Duration::from_millis(config.interval_ms as u64 + 5)).await;
        }
    }

    pub async fn run(&'static mut self, sd: &'static Softdevice) -> ! {
        self.ble_event_sender.send(BleEvent::Idle).await;

        let adv_data: LegacyAdvertisementPayload = LegacyAdvertisementBuilder::new()
            .flags(&[Flag::GeneralDiscovery, Flag::LE_Only])
            //.services_16(ServiceList::Complete, &[ServiceUuid16::BATTERY])
            .full_name("SimpleBLE DUT")
            .build();

        let scan_data: LegacyAdvertisementPayload = LegacyAdvertisementBuilder::new()
            //.services_128(ServiceList::Complete, &[0x9e7312e0_2354_11eb_9f10_fbc30a62cf38_u128.to_le_bytes()])
            .build();

        loop {
            let config = peripheral::Config::default();
            let adv = peripheral::ConnectableAdvertisement::ScannableUndirected {
                adv_data: &adv_data,
                scan_data: &scan_data,
            };

            self.ble_event_sender.send(BleEvent::Advertising).await;

            let conn = unwrap!(peripheral::advertise_connectable(sd, adv, &config).await);

            self.ble_event_sender.send(BleEvent::Connected).await;

            let fut_notify_stream = self.handle_notify_stream(&self.server, &conn, self.notify_stream_channel.receiver());
            let fut_indicate_stream = self.handle_indicate_stream(&self.server, &conn, self.indicate_stream_channel.receiver());

            let fut_gatt_server = gatt_server::run(&conn, &self.server, |e| match e {
                GattServerEvent::Dut(e) => match e {
                    DutGattServiceEvent::ControlWrite(val) => {
                        info!("wrote control: {}", val);
                        unwrap!(self.server.dut.status_set(&val));
                        self.handle_control_write(val);
                    }
                    DutGattServiceEvent::NotifyStreamCccdWrite { notifications } => {
                        info!("notify stream notifications: {}", notifications);
                        *self.notify_stream_active.borrow_mut() = notifications;
                    }
                    DutGattServiceEvent::IndicateStreamCccdWrite { indications } => {
                        info!("indicate stream indications: {}", indications);
                        *self.indicate_stream_active.borrow_mut() = indications;
                    }
                },
            });

            match select3(fut_gatt_server, fut_notify_stream, fut_indicate_stream).await {
                // Handle completion of any of the futures
                Either3::First(e) => {
                    // Server error or disconnection
                    info!("gatt_server run exited with error: {:?}", e);
                }
                Either3::Second(_) => {
                    // Notify stream ended (shouldn't happen)
                    info!("notify stream ended unexpectedly");
                }
                Either3::Third(_) => {
                    // Indicate stream ended (shouldn't happen)
                    info!("indicate stream ended unexpectedly");
                }
            }

            self.ble_event_sender.send(BleEvent::Disconnected).await;
        }
    }
}

#[embassy_executor::task]
pub async fn softdevice_task(sd: &'static Softdevice) -> ! {
    sd.run().await
}

#[embassy_executor::task]
pub async fn comms_ble_task(controller: &'static mut BleController, sd: &'static Softdevice) -> ! {
    controller.run(sd).await
}
