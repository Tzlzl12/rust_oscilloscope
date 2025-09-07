#![allow(dead_code)]
use crate::oscilloscope::font::FONT6X8;

use stm32f4xx_hal::i2c::I2c;

use stm32f4xx_hal::i2c::Instance;

use defmt;

pub struct Point {
  pub x: u8,
  pub y: u8,
}
pub struct Oscilloscope<I2C>
where
  I2C: Instance,
{
  i2c: Option<I2c<I2C>>,
  dirty_page: u8,
  buffer: [[u8; 128]; 8],

  origin: Point,
  pub current_point: Point,
  pub previous_point: Point,
}

pub trait Draw {
  fn draw_pixel(&mut self, x: u8, y: u8);
}

impl<I2C> Draw for Oscilloscope<I2C>
where
  I2C: Instance,
{
  #[inline(always)]
  fn draw_pixel(&mut self, x: u8, y: u8) {
    let Point {
      x: origin_x,
      y: origin_y,
    } = self.get_origon();
    let offset = (y + origin_y) % 8;
    let page = (y + origin_y) / 8;
    self.buffer[page as usize][(x + origin_x) as usize] |= 1 << offset;
    self.dirty_page |= 1 << page;
  }
}
impl<I2C> Oscilloscope<I2C>
where
  I2C: Instance,
{
  const OLED_ADDR: u8 = 0x3c;
  const OLED_PAGE: u8 = 8;
  const OLED_WIDTH: u8 = 128;
  #[inline(always)]
  fn draw_pixels(&mut self, x: u8, y: u8, data: u8) {
    if y % 8 == 0 {
      self.buffer[y as usize / 8][x as usize] = data;
      self.dirty_page |= 1 << (y as usize / 8);
    } else {
      let offset = y as usize / 8;
      self.dirty_page |= 0x11 << offset;
      self.buffer[offset][x as usize] |= data >> (y % 8);
      self.buffer[offset + 1][x as usize] |= data << (8 - (y % 8));
    }
  }

  fn set_cursor(&mut self, page: u8, x: u8) {
    let cmd = [0x00, x & 0x0f, ((x >> 4) & 0x0f), (0xb0 | page)];
    if let Some(i2c) = &mut self.i2c {
      i2c.write(Self::OLED_ADDR, &cmd).ok();
    }
  }

  pub fn new(i2c: Option<I2c<I2C>>) -> Self {
    Self {
      i2c,
      dirty_page: 0,
      buffer: [[0; 128]; 8],

      origin: Point {
        x: Self::OLED_WIDTH / 2,
        y: Self::OLED_PAGE * 4,
      },
      current_point: Point { x: 0, y: 0 },
      previous_point: Point { x: 0, y: 0 },
    }
  }

  pub fn init(&mut self) {
    let cmds = [
      0xAE, // 关闭OLED
      0xD5, 0x80, // 设置显示时钟分频因子/振荡器频率
      0x20, 0x02, // 设置内存寻址模式
      0xA8, 0x3F, // 设置多路传输比率
      0xDA, 0x12, // 设置列引脚硬件配置
      /* ----- 方向显示配置 ----- */
      0xA1, // 设置段重映射 (0xA1 正常)
      0xC8, // 设置行输出扫描方向(0xC8 正常)
      /* ----- END ----- */
      0x40, // 设置屏幕起始行
      0xD3, 0x00, // 设置显示偏移(not offset)
      0x81, 0xCF, // 设置对比度
      0xD9, 0xF1, // 设置预充电期间的持续时间
      0xDB, 0x20, // 调整VCOMH调节器的输出
      0x8D, 0x14, // 电荷泵设置
      0xA4, // 全局显示开启(黑屏/亮屏) ON/OFF(A5)
      0xA6, // 设置显示方式(正常/反显)
      0xAF, // 打开 OLED 显示
    ];
    if let Some(i2c) = self.i2c.as_mut() {
      i2c.write(Self::OLED_ADDR, &cmds).ok();
    }
    self.dirty_page = 0xff;
    self.render();
    self.dirty_page = 0x00;
  }
  #[inline(always)]
  pub fn draw_char(&mut self, x: u8, y: u8, c: char) {
    const OFFSET: u8 = 0x20;
    let char: [u8; 6] = FONT6X8[(c as u8 - OFFSET) as usize];

    for i in 0..6 {
      self.draw_pixels(x + i, y, char[i as usize]);
    }
  }

  pub fn draw_string(&mut self, x: u8, y: u8, s: &str) {
    for (i, c) in s.chars().enumerate() {
      self.draw_char(x + (i * 6) as u8, y, c);
    }
  }
  fn oled_pow(&self, x: u8) -> u32 {
    let mut res = 1;
    for _ in 0..x {
      res *= 10;
    }
    res
  }
  pub fn draw_number(&mut self, x: u8, y: u8, n: u32, len: u8) {
    for i in 0..len {
      let num = n / self.oled_pow(len - i - 1) % 10;
      self.draw_char(x + (i * 6), y, (num as u8 + 0x30) as char);
    }
  }
  #[allow(unused)]
  pub fn show_image(&mut self, x: u8, y: u8, width: u8, height: u8, data: &[u8]) {
    let h = height / 8;
    for i in 0..h {
      for j in 0..width {
        self.draw_pixels(x + j, y + (i * 8), data[(i * width + j) as usize]);
      }
    }
  }

  /// 清空数据(oled render之后产生的数据)
  pub fn clear_data(&mut self) {
    for i in 0..8 {
      if self.dirty_page & (1 << i) != 0 {
        // 存在数据
        defmt::info!("clear page {}", i);
        for j in 0..128 {
          self.buffer[i][j] = 0;
        }
      }
    }
    self.render();
  }

  /// clear the screen
  pub fn clear_screen(&mut self) {
    for i in 0..8 {
      for j in 0..128 {
        self.buffer[i][j] = 0;
      }
    }
  }

  pub fn render(&mut self) {
    for page in 0..8 {
      if self.dirty_page & (1 << page) != 0 {
        // defmt::info!("{:?}", self.buffer[0]);
        self.set_cursor(page, 0);
        // 准备数据：控制字节 + 页面数据
        let mut data = [0u8; 129]; // 1 + 128 + 3 (maybe, if want integrate set_cursor command)
        data[0] = 0x40; // 数据模式
        data[1..].copy_from_slice(&self.buffer[page as usize]);
        if let Some(i2c) = self.i2c.as_mut() {
          i2c.write(Self::OLED_ADDR, &data).ok();
        }
      }
    }
    self.dirty_page = 0x00;
  }

  fn get_origon(&self) -> &Point {
    &self.origin
  }
}
