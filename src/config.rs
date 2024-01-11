use core::cell::RefCell;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::blocking_mutex::Mutex;
use embassy_sync::signal::Signal;
use usbd_hid::descriptor::{KeyboardReport, MediaKeyboardReport};

pub const CONFIG_OFFSET: u32 = 0xc000;
pub const SECTOR_SIZE: u32 = 16 * 1024;
pub static CONFIG: Mutex<CriticalSectionRawMutex, RefCell<Config<4>>> = {
  Mutex::new(RefCell::new(Config {
    types: [0u8; 4],
    keys: [0x68, 0x69, 0x6a, 0x6b],
    modifiers: [0u8; 4],
  }))
};
pub static UPDATE_SIGNAL: Signal<CriticalSectionRawMutex, ()> = Signal::new();

#[derive(Clone, Copy)]
pub struct Config<const N: usize> {
  pub types: [u8; N],
  pub keys: [u8; N],
  pub modifiers: [u8; N],
}

#[derive(Clone, Copy)]
pub enum Key {
  Keyboard(KeyboardReport),
  Media(MediaKeyboardReport),
  Unknown,
}

impl<const N: usize> Config<N> {
  pub fn alloc_buf(&self) -> [u8; 3 * N] {
    [0u8; 3 * N]
  }

  pub fn update(&mut self, bytes: &[u8]) {
    assert_eq!(bytes.len(), 3 * N, "invalid bytes count");
    let (types, keys) = bytes.split_at(N);
    let (keys, modifiers) = keys.split_at(N);
    if types != [255; N] {
      self.types.copy_from_slice(types);
    }
    if keys != [255; N] {
      self.keys.copy_from_slice(keys);
    }
    if modifiers != [255; N] {
      self.modifiers.copy_from_slice(modifiers);
    }
  }

  pub fn to_bytes(self) -> [u8; 3 * N] {
    let mut result = [0u8; 3 * N];
    result[..N].copy_from_slice(&self.types);
    result[N..2 * N].copy_from_slice(&self.keys);
    result[2 * N..].copy_from_slice(&self.modifiers);
    result
  }

  pub fn get_key_report(&self, idx: usize) -> Key {
    match self.types[idx] {
      0 => Key::Keyboard(KeyboardReport {
        keycodes: [self.keys[idx], 0, 0, 0, 0, 0],
        leds: 0,
        modifier: self.modifiers[idx],
        reserved: 0,
      }),
      1 => Key::Media(MediaKeyboardReport {
        usage_id: self.keys[idx] as u16,
      }),
      _ => Key::Unknown,
    }
  }

  pub fn get_zero_report(&self, idx: usize) -> Key {
    match self.types[idx] {
      0 => Key::Keyboard(KeyboardReport {
        keycodes: [0, 0, 0, 0, 0, 0],
        leds: 0,
        modifier: 0,
        reserved: 0,
      }),
      1 => Key::Media(MediaKeyboardReport { usage_id: 0 }),
      _ => Key::Unknown,
    }
  }
}
