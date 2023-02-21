#![no_std]
#![no_main]
#![feature(type_alias_impl_trait)]
use core::cell::RefCell;

use core::str;
use embassy_executor::Spawner;
use embassy_rp::gpio::{AnyPin, Level, Output};
use embassy_rp::i2c::{Blocking, Config, I2c};
use embassy_rp::peripherals::I2C1;
use embassy_time::{Duration, Timer};

use is31fl3730::{Is31fl3730, DEFAULT_I2C_ADDRESS, SECONDARY_I2C_ADDRESS};
use static_cell::StaticCell;

use embassy_embedded_hal::shared_bus::blocking::i2c::I2cDevice;
use embassy_sync::blocking_mutex::{raw::NoopRawMutex, NoopMutex};

use {defmt_rtt as _, panic_probe as _};

const OFF_PULSE_RATIO: u64 = 12;
const ON_PULSE_RATIO: u64 = 1;
const BASE_PULSE_WIDTH: u64 = 20;

type Ltp305I2cDevice = I2cDevice<'static, NoopRawMutex, I2c<'static, I2C1, Blocking>>;

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    defmt::info!("Starting Cup Simulator!");

    let p = embassy_rp::init(Default::default());
    let mut led_red = Output::new(p.PIN_16, Level::Low);

    static I2C_BUS: StaticCell<NoopMutex<RefCell<I2c<'static, I2C1, Blocking>>>> =
        StaticCell::new();
    let i2c = embassy_rp::i2c::I2c::new_blocking(p.I2C1, p.PIN_15, p.PIN_14, Config::default());
    let i2c_bus = I2C_BUS.init(NoopMutex::new(RefCell::new(i2c)));

    let display = Display::new(I2cDevice::new(i2c_bus), I2cDevice::new(i2c_bus));

    spawner.spawn(count(display)).unwrap();
    spawner.spawn(blink(p.PIN_25.into())).unwrap();

    loop {
        led_red.set_high();
        Timer::after(Duration::from_millis(ON_PULSE_RATIO * BASE_PULSE_WIDTH)).await;

        led_red.set_low();
        Timer::after(Duration::from_millis(OFF_PULSE_RATIO * BASE_PULSE_WIDTH)).await;
    }
}

#[embassy_executor::task]
async fn count(mut display: Display<Ltp305I2cDevice, Ltp305I2cDevice>) {
    use core::fmt::Write;
    let mut buf = [0u8; 4];
    let mut buf = FourDigitWriter::new(&mut buf[..]);

    display.init().unwrap();
    let mut cnt: u16 = 0;
    loop {
        Timer::after(Duration::from_millis(75)).await;
        write!(&mut buf, "{}", cnt).unwrap();
        display.display(buf.as_str()).unwrap();
        cnt += 1;
        if cnt > 9999 {
            cnt = 0;
        }
    }
}

#[embassy_executor::task]
async fn blink(pin: AnyPin) {
    let mut led = Output::new(pin, Level::Low);
    loop {
        led.toggle();
        Timer::after(Duration::from_millis(250)).await;
    }
}

struct Display<I2c1, I2c2> {
    segment_1_2: Is31fl3730<I2c1>,
    segment_3_4: Is31fl3730<I2c2>,
}

impl<I2c1, I2c2, E> Display<I2c1, I2c2>
where
    I2c1: embedded_hal::blocking::i2c::Write<Error = E>,
    I2c2: embedded_hal::blocking::i2c::Write<Error = E>,
{
    pub fn new(i2c_device1: I2c1, i2c_device2: I2c2) -> Self {
        Display {
            segment_1_2: Is31fl3730::new(i2c_device1, DEFAULT_I2C_ADDRESS),
            segment_3_4: Is31fl3730::new(i2c_device2, SECONDARY_I2C_ADDRESS),
        }
    }

    pub fn init(&mut self) -> Result<(), E> {
        self.segment_1_2.init()?;
        self.segment_3_4.init()?;
        Ok(())
    }

    pub fn display(&mut self, text: &str) -> Result<(), E> {
        self.segment_3_4
            .set_character(0, text.chars().nth(0).unwrap())?;
        self.segment_3_4
            .set_character(5, text.chars().nth(1).unwrap())?;
        self.segment_1_2
            .set_character(0, text.chars().nth(2).unwrap())?;
        self.segment_1_2
            .set_character(5, text.chars().nth(3).unwrap())?;
        self.segment_1_2.show()?;
        self.segment_3_4.show()?;
        Ok(())
    }
}

// Buffer for ascii 4 digit number string which
// is right aligned. Leading digits are filled with zeros.
pub struct FourDigitWriter<'a> {
    buf: &'a mut [u8],
}

impl<'a> FourDigitWriter<'a> {
    pub fn new(buf: &'a mut [u8]) -> Self {
        FourDigitWriter { buf }
    }

    pub fn as_str(&self) -> &str {
        str::from_utf8(&self.buf[0..self.capacity()]).unwrap()
    }

    #[inline]
    pub fn capacity(&self) -> usize {
        self.buf.len()
    }
}

impl core::fmt::Write for FourDigitWriter<'_> {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let cap = self.capacity();
        let idx = cap - s.as_bytes().len();
        self.buf[0..idx].fill(48);

        for (i, &b) in self.buf[idx..cap].iter_mut().zip(s.as_bytes().iter()) {
            *i = b;
        }
        Ok(())
    }
}
