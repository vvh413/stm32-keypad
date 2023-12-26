#![no_std]
#![no_main]

mod buttons;
mod handlers;

use buttons::Buttons;
use defmt::*;
use embassy_executor::Spawner;
use embassy_stm32::rcc::*;
use embassy_stm32::time::Hertz;
use embassy_stm32::usb_otg::Driver;
use embassy_stm32::{bind_interrupts, peripherals, usb_otg, Config, Peripherals};
use embassy_time::Timer;
use embassy_usb::class::hid::{HidReaderWriter, State};
use embassy_usb::Builder;
use futures::future::join3;
use handlers::{MyDeviceHandler, MyRequestHandler};
use usbd_hid::descriptor::{KeyboardReport, SerializedDescriptor};
use {defmt_rtt as _, panic_probe as _};

bind_interrupts!(struct Irqs {
    OTG_FS => usb_otg::InterruptHandler<peripherals::USB_OTG_FS>;
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

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
  let p = init();

  let mut ep_out_buffer = [0u8; 256];
  let mut config = embassy_stm32::usb_otg::Config::default();
  config.vbus_detection = false;
  let driver = Driver::new_fs(p.USB_OTG_FS, Irqs, p.PA12, p.PA11, &mut ep_out_buffer, config);

  let mut config = embassy_usb::Config::new(0x7668, 0x0001);
  config.manufacturer = Some("vvh413");
  config.product = Some("stm32 keypad");

  let mut device_descriptor = [0; 256];
  let mut config_descriptor = [0; 256];
  let mut bos_descriptor = [0; 256];
  let mut control_buf = [0; 64];

  let request_handler = MyRequestHandler {};
  let mut device_handler = MyDeviceHandler::new();
  let mut state = State::new();

  let mut builder = Builder::new(
    driver,
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
  let hid = HidReaderWriter::<_, 1, 8>::new(&mut builder, &mut state, config);
  let mut usb = builder.build();
  let usb_fut = usb.run();

  let mut buttons = Buttons::new(p.ADC1, p.PA1);

  let (reader, mut writer) = hid.split();

  let in_fut = async {
    info!("Waiting for writer to be ready");
    writer.ready().await;
    loop {
      if let Some(key) = buttons.get_key() {
        let report = KeyboardReport {
          keycodes: [key, 0, 0, 0, 0, 0],
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
