#![deny(unsafe_code)]
#![no_main]
#![no_std]

// Print panic message to probe console
use {defmt_rtt as _, panic_probe as _};
mod oscilloscope;

#[rtic::app(device = stm32f4xx_hal::pac, peripherals = true, dispatchers = [TIM2])]
mod app {
  use stm32f4xx_hal::{
    adc::{self, config::AdcConfig, Adc},
    dma::{config::DmaConfig, PeripheralToMemory, Stream0, StreamsTuple, Transfer},
    i2c::I2c,
    pac::{ADC1, DMA2},
    prelude::*,
  };

  use crate::oscilloscope::Oscilloscope;

  #[shared]
  struct Shared {
    transfer: Transfer<Stream0<DMA2>, 0, Adc<ADC1>, PeripheralToMemory, &'static mut [u16; 16]>,
  }

  // Local resources go here
  #[local]
  struct Local {
    new_buf: Option<&'static mut [u16; 16]>,
  }

  #[monotonic(binds = SysTick, default = true)]
  type Mono = systick_monotonic::Systick<1000>;

  #[init]
  fn init(ctx: init::Context) -> (Shared, Local, init::Monotonics) {
    let cp = ctx.core;
    let dp = ctx.device;

    let clocks = dp
      .RCC
      .constrain()
      .cfgr
      .use_hse(8.MHz())
      .sysclk(72.MHz())
      .freeze();

    let gpioa = dp.GPIOA.split();

    let gpiob = dp.GPIOB.split();

    let scl = gpiob.pb6;
    let sda = gpiob.pb7;
    let i2c = I2c::new(dp.I2C1, (scl, sda), 100.kHz(), &clocks);

    let oscilloscope = Oscilloscope::new(Some(i2c));

    let analog = gpioa.pa0.into_analog();

    let mut adc = Adc::adc1(
      dp.ADC1,
      true,
      AdcConfig::default().dma(adc::config::Dma::Continuous),
    );
    adc.configure_channel(
      &analog,
      adc::config::Sequence::One,
      adc::config::SampleTime::Cycles_480,
    );
    adc.enable();

    let streams2 = StreamsTuple::new(dp.DMA2);
    let buf = cortex_m::singleton!(:[u16; 16] = [0u16;16]).unwrap();
    let new_buf = cortex_m::singleton!(:[u16; 16] = [0u16;16]).unwrap();

    let transfer = Transfer::init_peripheral_to_memory(
      streams2.0,
      adc,
      buf,
      None,
      DmaConfig::default()
        .memory_increment(true)
        .transfer_complete_interrupt(true),
    );

    (
      Shared {
        // Initialization of shared resources go here
        transfer,
      },
      Local {
        // Initialization of local resources go here
        new_buf: Some(new_buf),
      },
      init::Monotonics(systick_monotonic::Systick::new(
        cp.SYST,
        clocks.hclk().to_Hz(),
      )),
    )
  }

  // Optional idle, can be removed if not needed.
  #[idle]
  fn idle(_: idle::Context) -> ! {
    loop {
      continue;
    }
  }

  #[task(shared = [transfer])]
  fn sample_data(ctx: sample_data::Context) {
    let mut transfer = ctx.shared.transfer;
    transfer.lock(|transfer| {
      transfer.start(|adc| {
        adc.start_conversion();
      });
    })
  }
  #[task(binds = DMA2_STREAM0)]
  fn dma2_stream0(ctx: dma2_stream0::Context) {}
}
