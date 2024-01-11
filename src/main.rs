#![no_std]
#![no_main]
#![feature(type_alias_impl_trait)]
#![feature(generic_const_exprs)]
#![allow(incomplete_features)]

mod buttons;
mod config;
mod handlers;

use config::{Key, CONFIG, CONFIG_OFFSET, SECTOR_SIZE, UPDATE_SIGNAL};
use embassy_executor::Spawner;
use embassy_futures::join::join;
use embassy_stm32::flash::Flash;
use embassy_stm32::gpio::{AnyPin, Output, Pin};
use embassy_stm32::rcc::*;
use embassy_stm32::time::Hertz;
use embassy_stm32::usb_otg::Driver;
use embassy_stm32::{bind_interrupts, flash, peripherals, usb_otg, Config, Peripherals};
use embassy_time::Timer;
use embassy_usb::class::hid::{HidReaderWriter, HidWriter, State};
use embassy_usb::Builder;
use handlers::{CustomRequestHandler, DeviceHandler};
use usbd_hid::descriptor::{KeyboardReport, MediaKeyboardReport, SerializedDescriptor};
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
async fn config_flash_task(mut flash: Flash<'static>, led: AnyPin) {
  let mut led = Output::new(led, embassy_stm32::gpio::Level::High, embassy_stm32::gpio::Speed::Low);
  CONFIG.lock(|config| {
    let mut config = config.borrow_mut();
    let mut data = config.alloc_buf();
    flash.read(CONFIG_OFFSET, &mut data).unwrap();
    config.update(&data);
  });
  loop {
    UPDATE_SIGNAL.wait().await;
    led.toggle();
    let config_bytes = CONFIG.lock(|config| config.borrow().to_bytes());
    flash.erase(CONFIG_OFFSET, CONFIG_OFFSET + SECTOR_SIZE).await.unwrap();
    flash.write(CONFIG_OFFSET, &config_bytes).await.unwrap();
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
  spawner.spawn(config_flash_task(flash, p.PC13.degrade())).unwrap();

  let mut config = embassy_usb::Config::new(0x7668, 0x0001);
  config.manufacturer = Some("vvh413");
  config.product = Some("stm32 keypad");
  config.serial_number = Some("00000001");

  config.device_class = 0xEF;
  config.device_sub_class = 0x02;
  config.device_protocol = 0x01;
  config.composite_with_iads = true;

  let mut device_descriptor = [0; 256];
  let mut config_descriptor = [0; 256];
  let mut bos_descriptor = [0; 256];
  let mut control_buf = [0; 64];

  let request_handler = CustomRequestHandler {};
  let mut device_handler = DeviceHandler::new();
  let mut state_kb = State::new();
  let mut state_media = State::new();

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

  let config_kb = embassy_usb::class::hid::Config {
    report_descriptor: KeyboardReport::desc(),
    request_handler: Some(&request_handler),
    poll_ms: 1,
    max_packet_size: 64,
  };
  let hid_kb = HidReaderWriter::<_, 12, 8>::new(&mut builder, &mut state_kb, config_kb);

  let config_media = embassy_usb::class::hid::Config {
    report_descriptor: MediaKeyboardReport::desc(),
    request_handler: None,
    poll_ms: 1,
    max_packet_size: 64,
  };
  let mut writer_media = HidWriter::<_, 8>::new(&mut builder, &mut state_media, config_media);

  let mut usb = builder.build();
  let usb_fut = async {
    loop {
      usb.run_until_suspend().await;
      usb.wait_resume().await;
    }
  };

  let mut buttons = buttons::Buttons::new(p.ADC1, p.PA1);

  let (reader, mut writer_kb) = hid_kb.split();

  let in_fut = async {
    loop {
      writer_kb.ready().await;
      let report = match buttons.get_state() {
        buttons::State::Rising(idx) => CONFIG.lock(|config| config.borrow().get_key_report(idx)),
        buttons::State::Falling(idx) => CONFIG.lock(|config| config.borrow().get_zero_report(idx)),
        buttons::State::None => Key::Unknown,
      };
      match report {
        Key::Keyboard(report) => writer_kb.write_serialize(&report).await.unwrap(),
        Key::Media(report) => writer_media.write_serialize(&report).await.unwrap(),
        Key::Unknown => Timer::after_millis(1).await,
      };
    }
  };

  let out_fut = async {
    reader.run(false, &request_handler).await;
  };

  join(usb_fut, join(in_fut, out_fut)).await;
}
