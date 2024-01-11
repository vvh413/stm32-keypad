use core::sync::atomic::{AtomicBool, Ordering};
use defmt::info;
use embassy_usb::class::hid::{ReportId, RequestHandler};
use embassy_usb::control::OutResponse;
use embassy_usb::Handler;

use crate::config::{CONFIG, UPDATE_SIGNAL};

pub struct CustomRequestHandler {}

impl RequestHandler for CustomRequestHandler {
  fn get_report(&self, id: ReportId, _buf: &mut [u8]) -> Option<usize> {
    info!("Get report for {:?}", id);
    None
  }

  fn set_report(&self, id: ReportId, data: &[u8]) -> OutResponse {
    info!("Set report for {:?}: {=[u8]}", id, data);
    if data.len() == 12 {
      CONFIG.lock(|keys| {
        keys.borrow_mut().update(data);
      });
      UPDATE_SIGNAL.signal(());
    }
    OutResponse::Accepted
  }

  fn set_idle_ms(&self, id: Option<ReportId>, dur: u32) {
    info!("Set idle rate for {:?} to {:?}", id, dur);
  }

  fn get_idle_ms(&self, id: Option<ReportId>) -> Option<u32> {
    info!("Get idle rate for {:?}", id);
    None
  }
}

pub struct DeviceHandler {
  configured: AtomicBool,
}

impl DeviceHandler {
  pub fn new() -> Self {
    DeviceHandler {
      configured: AtomicBool::new(false),
    }
  }
}

impl Handler for DeviceHandler {
  fn enabled(&mut self, enabled: bool) {
    self.configured.store(false, Ordering::Relaxed);
    if enabled {
      info!("Device enabled");
    } else {
      info!("Device disabled");
    }
  }

  fn reset(&mut self) {
    self.configured.store(false, Ordering::Relaxed);
    info!("Bus reset, the Vbus current limit is 100mA");
  }

  fn addressed(&mut self, addr: u8) {
    self.configured.store(false, Ordering::Relaxed);
    info!("USB address set to: {}", addr);
  }

  fn configured(&mut self, configured: bool) {
    self.configured.store(configured, Ordering::Relaxed);
    if configured {
      info!("Device configured, it may now draw up to the configured current limit from Vbus.")
    } else {
      info!("Device is no longer configured, the Vbus current limit is 100mA.");
    }
  }
}
