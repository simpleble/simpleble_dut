macro_rules! define_channel {
    ($name:ident, $t:ty, $size:expr) => {
        paste::paste! {
            pub struct [<$name Channel>] {
                channel: &'static embassy_sync::channel::Channel<embassy_sync::blocking_mutex::raw::NoopRawMutex, $t, $size>,
            }

            mod [<__channel_types_ $name:lower>] {
                use super::*;
                pub type [<$name Sender>] = embassy_sync::channel::Sender<'static, embassy_sync::blocking_mutex::raw::NoopRawMutex, $t, $size>;
                pub type [<$name Receiver>] = embassy_sync::channel::Receiver<'static, embassy_sync::blocking_mutex::raw::NoopRawMutex, $t, $size>;
            }
            pub use [<__channel_types_ $name:lower>]::*;

            impl [<$name Channel>] {
                pub fn new() -> Self {
                    static CHANNEL: static_cell::StaticCell<embassy_sync::channel::Channel<embassy_sync::blocking_mutex::raw::NoopRawMutex, $t, $size>> = static_cell::StaticCell::new();
                    Self {
                        channel: &*CHANNEL.init(embassy_sync::channel::Channel::new())
                    }
                }

                pub fn sender(&self) -> [<$name Sender>] {
                    self.channel.sender()
                }

                pub fn receiver(&self) -> [<$name Receiver>] {
                    self.channel.receiver()
                }
            }
        }
    };
}

pub(crate) use define_channel;