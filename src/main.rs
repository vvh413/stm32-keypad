#![no_std]
#![no_main]
#![feature(type_alias_impl_trait)]

mod buttons;
mod handlers;

use buttons::{Buttons, KEYS, KEYS_OFFSET, KEYS_SIGNAL, SECTOR_SIZE};
use defmt::*;
use embassy_executor::Spawner;
use embassy_stm32::flash::Flash;
use embassy_stm32::gpio::{AnyPin, Output, Pin};
use embassy_stm32::rcc::*;
use embassy_stm32::time::Hertz;
use embassy_stm32::usb_otg::Driver;
use embassy_stm32::{bind_interrupts, flash, peripherals, usb_otg, Config, Peripherals};
use embassy_time::Timer;
use embassy_usb::class::hid::{HidReaderWriter, State};
use embassy_usb::Builder;
use futures::future::join3;
use handlers::{CustomRequestHandler, DeviceHandler};
use usbd_hid::descriptor::{KeyboardReport, SerializedDescriptor};
use {defmt_rtt as _, panic_probe as _};

bind_interrupts!(struct Irqs {
  OTG_FS => usb_otg::InterruptHandler<peripherals::USB_OTG_FS>;
  FLASH => flash::InterruptHandler;
});

fn init() -> Peripherals {
  let mut config = Config::default();
  {
    config.rcc.hse = Some(Hse {
      freq: Hertz(25_000_000),
      mode: HseMode::Oscillator,
    });
    config.rcc.pll_src = PllSource::HSE;
    config.rcc.pll = Some(Pll {
      prediv: PllPreDiv::DIV25,
      mul: PllMul::MUL192,
      divp: Some(PllPDiv::DIV2),
      divq: Some(PllQDiv::DIV4),
      divr: None,
    });
    config.rcc.ahb_pre = AHBPrescaler::DIV1;
    config.rcc.apb1_pre = APBPrescaler::DIV2;
    config.rcc.apb2_pre = APBPrescaler::DIV1;
    config.rcc.sys = Sysclk::PLL1_P;
  }
  embassy_stm32::init(config)
}

#[embassy_executor::task]
async fn flash_task(mut flash: Flash<'static>, led: AnyPin) {
  let mut led = Output::new(led, embassy_stm32::gpio::Level::High, embassy_stm32::gpio::Speed::Low);
  KEYS.lock(|keys| {
    let mut data = [0u8; 4];
    flash.read(KEYS_OFFSET, &mut data).unwrap();
    if data != [255; 4] {
      keys.replace(data);
    }
  });
  loop {
    KEYS_SIGNAL.wait().await;
    led.toggle();
    let keys = KEYS.lock(|keys| *keys.borrow());
    flash.erase(KEYS_OFFSET, KEYS_OFFSET + SECTOR_SIZE).await.unwrap();
    flash.write(KEYS_OFFSET, &keys).await.unwrap();
    led.toggle();
  }
}

#[embassy_executor::main]
async fn main(spawner: Spawner) {
  let p = init();

  let mut ep_out_buffer = [0u8; 256];
  let mut config = usb_otg::Config::default();
  config.vbus_detection = false;
  let otg_fs_driver = Driver::new_fs(p.USB_OTG_FS, Irqs, p.PA12, p.PA11, &mut ep_out_buffer, config);

  let flash = Flash::new(p.FLASH, Irqs);
  spawner.spawn(flash_task(flash, p.PC13.degrade())).unwrap();

  let mut config = embassy_usb::Config::new(0x7668, 0x0001);
  config.manufacturer = Some("vvh413");
  config.product = Some("stm32 keypad");

  let mut device_descriptor = [0; 256];
  let mut config_descriptor = [0; 256];
  let mut bos_descriptor = [0; 256];
  let mut control_buf = [0; 64];

  let request_handler = CustomRequestHandler {};
  let mut device_handler = DeviceHandler::new();
  let mut state = State::new();

  let mut builder = Builder::new(
    otg_fs_driver,
    config,
    &mut device_descriptor,
    &mut config_descriptor,
    &mut bos_descriptor,
    &mut [],
    &mut control_buf,
  );
  builder.handler(&mut device_handler);

  let config = embassy_usb::class::hid::Config {
    report_descriptor: KeyboardReport::desc(),
    request_handler: Some(&request_handler),
    poll_ms: 1,
    max_packet_size: 64,
  };
  let hid = HidReaderWriter::<_, 4, 8>::new(&mut builder, &mut state, config);
  let mut usb = builder.build();
  let usb_fut = usb.run();

  let mut buttons = Buttons::new(p.ADC1, p.PA1);

  let (reader, mut writer) = hid.split();

  let in_fut = async {
    info!("Waiting for writer to be ready");
    writer.ready().await;
    loop {
      if let Some(idx) = buttons.get_pressed() {
        let keys = KEYS.lock(|keys| *keys.borrow());
        info!("keys {}", keys);
        let report = KeyboardReport {
          keycodes: [keys[idx], 0, 0, 0, 0, 0],
          leds: 0,
          modifier: 0,
          reserved: 0,
        };
        writer.write_serialize(&report).await.unwrap();
      }

      if buttons.all_released() {
        let report = KeyboardReport {
          keycodes: [0, 0, 0, 0, 0, 0],
          leds: 0,
          modifier: 0,
          reserved: 0,
        };
        writer.write_serialize(&report).await.unwrap();
      }

      Timer::after_millis(5).await;
    }
  };

  let out_fut = async {
    reader.run(false, &request_handler).await;
  };

  join3(usb_fut, in_fut, out_fut).await;
}
