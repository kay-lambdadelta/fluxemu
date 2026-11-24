use core::{
    ffi::c_int,
    ptr,
    sync::atomic::{AtomicPtr, Ordering},
};

use crate::{
    sys::{
        CONFIG_ESPRESSIF_SPI2_MISOPIN, ENODEV, ESPRESSIF_SPI2, OUTPUT, SIG_GPIO_OUT_IDX,
        esp_configgpio, esp_gpio_matrix_out, esp_gpiowrite, esp_spibus_initialize, lcd_dev_s,
        nxsched_usleep, spi_dev_s, st7735_lcdinitialize,
    },
    syslog,
};

// These values are taken from NuttX, and not the datasheet, assuming NuttX is right about them
const DATA_COMMAND_PIN: c_int = CONFIG_ESPRESSIF_SPI2_MISOPIN as c_int;
const RESET_PIN: c_int = 32;
const RESET_PULSE_MICROSECONDS: u32 = 10_000;
const RESET_RECOVERY_MICROSECONDS: u32 = 120_000;
const DISPLAY_NUMBER: c_int = 0;

static SPI_BUS: AtomicPtr<spi_dev_s> = AtomicPtr::new(ptr::null_mut());
static DISPLAY: AtomicPtr<lcd_dev_s> = AtomicPtr::new(ptr::null_mut());

fn configure_output(pin: c_int, level: bool) {
    unsafe {
        esp_gpiowrite(pin, level);
        esp_configgpio(pin, OUTPUT as u16);
        esp_gpio_matrix_out(pin as u32, SIG_GPIO_OUT_IDX, false, false);
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn board_lcd_initialize() -> c_int {
    let spi_bus = unsafe { esp_spibus_initialize(ESPRESSIF_SPI2 as c_int) };
    if spi_bus.is_null() {
        let error = -(ENODEV as c_int);
        syslog::error(c"Failed to initialize the display SPI bus", error);
        return error;
    }

    SPI_BUS.store(spi_bus, Ordering::Release);

    configure_output(DATA_COMMAND_PIN, true);
    configure_output(RESET_PIN, false);
    unsafe { nxsched_usleep(RESET_PULSE_MICROSECONDS) };
    unsafe { esp_gpiowrite(RESET_PIN, true) };
    unsafe { nxsched_usleep(RESET_RECOVERY_MICROSECONDS) };

    0
}

#[unsafe(no_mangle)]
pub extern "C" fn board_lcd_getdev(display_number: c_int) -> *mut lcd_dev_s {
    if display_number != DISPLAY_NUMBER {
        return ptr::null_mut();
    }

    let display = DISPLAY.load(Ordering::Acquire);
    if !display.is_null() {
        return display;
    }

    let spi_bus = SPI_BUS.load(Ordering::Acquire);
    if spi_bus.is_null() {
        return ptr::null_mut();
    }

    let display = unsafe { st7735_lcdinitialize(spi_bus) };
    if display.is_null() {
        syslog::error(c"Failed to initialize the ST7735", -(ENODEV as c_int));
        return display;
    }
    DISPLAY.store(display, Ordering::Release);

    display
}

#[unsafe(no_mangle)]
pub extern "C" fn board_lcd_uninitialize() {
    let display = DISPLAY.load(Ordering::Acquire);
    if display.is_null() {
        return;
    }

    if let Some(set_power) = unsafe { (*display).setpower } {
        unsafe { set_power(display, 0) };
    }
}
