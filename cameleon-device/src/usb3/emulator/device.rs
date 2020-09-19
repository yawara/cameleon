use crate::usb3::{DeviceInfo, Result};

use super::{
    channel::{ControlChannel, ReceiveChannel},
    emulator_impl::{DeviceHandle, IfaceKind},
};

pub struct Device {
    device_id: u32,
    device_info: DeviceInfo,
}

impl Device {
    pub fn control_channel(&self) -> Result<ControlChannel> {
        let handle = DeviceHandle::new(self.device_id, IfaceKind::Control);
        Ok(ControlChannel::new(handle))
    }

    pub fn event_channel(&self) -> Result<Option<ReceiveChannel>> {
        let handle = DeviceHandle::new(self.device_id, IfaceKind::Event);
        Ok(Some(ReceiveChannel::new(handle)))
    }

    pub fn stream_channel(&self) -> Result<Option<ReceiveChannel>> {
        let handle = DeviceHandle::new(self.device_id, IfaceKind::Stream);
        Ok(Some(ReceiveChannel::new(handle)))
    }

    pub fn device_info(&self) -> &DeviceInfo {
        &self.device_info
    }

    pub(super) fn new(device_id: u32, device_info: DeviceInfo) -> Self {
        let device = Self {
            device_id,
            device_info,
        };

        log::info! {"{}: create device", device.log_name()};

        device
    }

    //TODO: We need logger.
    fn log_name(&self) -> String {
        format!(
            "{}-{}-{}",
            self.device_info.vendor_name,
            self.device_info.model_name,
            self.device_info.serial_number,
        )
    }
}