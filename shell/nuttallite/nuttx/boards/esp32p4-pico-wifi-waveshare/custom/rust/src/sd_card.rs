use core::ptr;

use crate::{
    sys::{esp_ldo_channel_acquire, esp_ldo_config_t, esp_mmcsd_spi_initialize},
    syslog,
};

const LDO_CHANNEL: u8 = 4;
const LDO_VOLTAGE_MILLIVOLTS: u16 = 3300;

static mut LDO_CONFIG: esp_ldo_config_t = esp_ldo_config_t {
    chan_id: LDO_CHANNEL,
    voltage_mv: LDO_VOLTAGE_MILLIVOLTS,
    handler: ptr::null_mut(),
};

pub fn initialize() {
    let result = unsafe { esp_ldo_channel_acquire(&raw mut LDO_CONFIG) };
    if result < 0 {
        syslog::error(c"Failed to enable the SD card LDO", result);
        return;
    }

    let result = unsafe { esp_mmcsd_spi_initialize() };
    if result < 0 {
        syslog::error(c"Failed to initialize the SD card slot", result);
    }
}
