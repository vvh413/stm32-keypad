use core::cell::RefCell;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::blocking_mutex::Mutex;
use embassy_sync::signal::Signal;

pub const CONFIG_OFFSET: u32 = 0xc000;
pub const SECTOR_SIZE: u32 = 16 * 1024;
pub static CONFIG: Mutex<CriticalSectionRawMutex, RefCell<Config<4>>> = {
  Mutex::new(RefCell::new(Config {
    keys: [0x68, 0x69, 0x6a, 0x6b],
    modifiers: [0u8; 4],
  }))
};
pub static UPDATE_SIGNAL: Signal<CriticalSectionRawMutex, ()> = Signal::new();

#[derive(Clone, Copy)]
pub struct Config<const N: usize> {
  pub keys: [u8; N],
  pub modifiers: [u8; N],
}

impl<const N: usize> Config<N> {
  pub fn alloc_buf(&self) -> [u8; 2 * N] {
    [0u8; 2 * N]
  }

  pub fn update(&mut self, bytes: &[u8]) {
    assert_eq!(bytes.len(), 2 * N, "invalid bytes count");
    let (keys, modifiers) = bytes.split_at(N);
    if keys != [255; N] {
      self.keys.copy_from_slice(keys);
    }
    if modifiers != [255; N] {
      self.modifiers.copy_from_slice(modifiers);
    }
  }

  pub fn to_bytes(self) -> [u8; 2 * N] {
    let mut result = [0u8; 2 * N];
    result[..N].copy_from_slice(&self.keys);
    result[N..].copy_from_slice(&self.modifiers);
    result
  }
}
