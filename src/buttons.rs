use core::cell::RefCell;
use core::ops::RangeInclusive;

use debouncr::{debounce_8, Debouncer, Repeat8};
use defmt::*;
use embassy_stm32::adc::{Adc, AdcPin};
use embassy_stm32::gpio::Pin;
use embassy_stm32::peripherals::ADC1;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::blocking_mutex::Mutex;
use embassy_sync::signal::Signal;
use embassy_time::Delay;

pub const KEYS_OFFSET: u32 = 0xc000;
pub const SECTOR_SIZE: u32 = 16 * 1024;
pub static KEYS: Mutex<CriticalSectionRawMutex, RefCell<[u8; 4]>> = Mutex::new(RefCell::new([0x68, 0x69, 0x6a, 0x6b]));
pub static KEYS_SIGNAL: Signal<CriticalSectionRawMutex, ()> = Signal::new();
const THRESHOLDS: [RangeInclusive<u16>; 4] = [700..=1000, 1500..=1800, 2300..=2600, 3200..=3500];

pub struct Buttons<'a, P>
where
  P: Pin + AdcPin<ADC1>,
{
  adc: Adc<'a, ADC1>,
  pin: P,
  debouncers: [Debouncer<u8, Repeat8>; THRESHOLDS.len()],
}

impl<'a, P> Buttons<'a, P>
where
  P: Pin + AdcPin<ADC1>,
{
  pub fn new(adc_pin: ADC1, pin: P) -> Buttons<'a, P> {
    let mut delay = Delay;
    let adc = Adc::new(adc_pin, &mut delay);
    let debouncers = [0; THRESHOLDS.len()].map(|_| debounce_8(false));
    Self { adc, pin, debouncers }
  }

  fn read(&mut self) -> u16 {
    self.adc.read(&mut self.pin)
  }

  pub fn get_pressed(&mut self) -> Option<usize> {
    let value = self.read();
    // debug!("analog value: {}", value);
    let mut idx = None;
    for (i, threshold) in THRESHOLDS.iter().enumerate() {
      match self.debouncers[i].update(threshold.contains(&value)) {
        Some(debouncr::Edge::Rising) => {
          info!("button #{} pressed", i);
          idx = Some(i);
        }
        Some(debouncr::Edge::Falling) => {
          info!("button #{} released", i)
        }
        None => {}
      }
    }
    idx
  }

  pub fn all_released(&self) -> bool {
    self.debouncers.iter().all(|debouncer| debouncer.is_low())
  }
}
