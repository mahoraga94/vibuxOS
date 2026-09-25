use bootloader_api::info::{FrameBuffer, FrameBufferInfo, PixelFormat};

use core::fmt::{self, Write};

const FG: (u8, u8, u8) = (190, 230, 160);

static FONT: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/vibux-font.psf"));

#[derive(Clone, Copy)]
struct Font {
    offset: usize,
    glyph_size: usize,
    glyph_count: usize,
    width: usize,
    height: usize,
}

impl Font {
    fn parse(data: &[u8]) -> Self {
        if data.len() >= 4 && data[0] == 0x36 && data[1] == 0x04 {
            let mode = data[2];

            let glyph_size = data[3] as usize;

            let glyph_count = if mode & 0x02 != 0 { 512 } else { 256 };

            return Self {
                offset: 4,
                glyph_size,
                glyph_count,
                width: 8,
                height: glyph_size,
            };
        }

        if data.len() >= 32 && read_u32(data, 0) == 0x864A_B572 {
            return Self {
                offset: read_u32(data, 8) as usize,

                glyph_count: read_u32(data, 16) as usize,

                glyph_size: read_u32(data, 20) as usize,

                width: read_u32(data, 28) as usize,

                height: read_u32(data, 24) as usize,
            };
        }

        panic!("unsupported PSF font");
    }

    fn row_bytes(&self) -> usize {
        self.width.saturating_add(7) / 8
    }

    fn glyph(&self, character: u8) -> &'static [u8] {
        let index = character as usize;

        let index = if index < self.glyph_count { index } else { 0 };

        let start = self
            .offset
            .saturating_add(index.saturating_mul(self.glyph_size));

        let end = start.saturating_add(self.glyph_size);

        if end > FONT.len() {
            return &[];
        }

        &FONT[start..end]
    }
}

pub struct FrameTerminal<'a> {
    buffer: &'a mut [u8],
    info: FrameBufferInfo,
    font: Font,
    x: usize,
    y: usize,
    cell_w: usize,
    cell_h: usize,
    mirror_serial: bool,
    ansi_state: u8,
    csi_params: [u16; 8],
    csi_count: usize,
    csi_value: u16,
    csi_have_value: bool,
    csi_private: bool,
    saved_x: usize,
    saved_y: usize,
}

impl<'a> FrameTerminal<'a> {
    pub fn new(framebuffer: &'a mut FrameBuffer) -> Self {
        let info = framebuffer.info();

        let buffer = framebuffer.buffer_mut();

        let font = Font::parse(FONT);

        let cell_w = font.width + 1;

        let cell_h = font.height + 1;

        let mut terminal = Self {
            buffer,
            info,
            font,
            x: 0,
            y: 0,
            cell_w,
            cell_h,
            mirror_serial: false,
            ansi_state: 0,
            csi_params: [0; 8],
            csi_count: 0,
            csi_value: 0,
            csi_have_value: false,
            csi_private: false,
            saved_x: 0,
            saved_y: 0,
        };

        terminal.clear();

        terminal
    }

    pub fn set_serial_mirror(&mut self, enabled: bool) {
        self.mirror_serial = enabled;
    }

    pub fn write_bytes(&mut self, bytes: &[u8]) {
        let old_mirror = self.mirror_serial;
        self.mirror_serial = false;

        for &byte in bytes {
            match self.ansi_state {
                0 => match byte {
                    0x1B => self.ansi_state = 1,
                    0x08 | 0x7F => self.backspace(),
                    0x07 => {}
                    _ => self.put_char(byte),
                },

                1 => match byte {
                    b'[' => {
                        self.ansi_state = 2;
                        self.csi_params = [0; 8];
                        self.csi_count = 0;
                        self.csi_value = 0;
                        self.csi_have_value = false;
                        self.csi_private = false;
                    }

                    b']' => {
                        self.ansi_state = 3;
                    }

                    b'7' => {
                        self.saved_x = self.x;
                        self.saved_y = self.y;
                        self.ansi_state = 0;
                    }

                    b'8' => {
                        self.x = self.saved_x;
                        self.y = self.saved_y;
                        self.ansi_state = 0;
                    }

                    _ => {
                        self.ansi_state = 0;
                    }
                },

                2 => {
                    if byte == b'?' && !self.csi_have_value && self.csi_count == 0 {
                        self.csi_private = true;
                        continue;
                    }

                    if byte.is_ascii_digit() {
                        self.csi_value = self
                            .csi_value
                            .saturating_mul(10)
                            .saturating_add((byte - b'0') as u16);
                        self.csi_have_value = true;
                        continue;
                    }

                    if byte == b';' {
                        if self.csi_count < self.csi_params.len() {
                            self.csi_params[self.csi_count] = if self.csi_have_value {
                                self.csi_value
                            } else {
                                0
                            };
                            self.csi_count += 1;
                        }

                        self.csi_value = 0;
                        self.csi_have_value = false;
                        continue;
                    }

                    if (0x40..=0x7E).contains(&byte) {
                        if self.csi_count < self.csi_params.len() {
                            self.csi_params[self.csi_count] = if self.csi_have_value {
                                self.csi_value
                            } else {
                                0
                            };
                            self.csi_count += 1;
                        }

                        self.finish_csi(byte);
                        self.ansi_state = 0;
                        continue;
                    }

                    self.ansi_state = 0;
                }

                3 => {
                    if byte == 0x07 {
                        self.ansi_state = 0;
                    } else if byte == 0x1B {
                        self.ansi_state = 4;
                    }
                }

                4 => {
                    if byte == b'\\' {
                        self.ansi_state = 0;
                    } else if byte == 0x1B {
                        self.ansi_state = 4;
                    } else {
                        self.ansi_state = 3;
                    }
                }

                _ => {
                    self.ansi_state = 0;
                }
            }
        }

        self.mirror_serial = old_mirror;
    }

    fn csi_param(&self, index: usize, default: usize) -> usize {
        if index >= self.csi_count {
            return default;
        }

        let value = self.csi_params[index] as usize;

        if value == 0 { default } else { value }
    }

    fn columns(&self) -> usize {
        (self.info.width / self.cell_w).max(1)
    }

    fn rows(&self) -> usize {
        (self.info.height / self.cell_h).max(1)
    }

    fn cursor_position(&mut self, row: usize, column: usize) {
        self.y = row
            .saturating_sub(1)
            .min(self.rows().saturating_sub(1))
            .saturating_mul(self.cell_h);

        self.x = column
            .saturating_sub(1)
            .min(self.columns().saturating_sub(1));
    }

    fn erase_line_range(&mut self, start: usize, end: usize) {
        let start = start.min(self.columns());
        let end = end.min(self.columns());

        if start >= end {
            return;
        }

        for column in start..end {
            self.clear_cell(column.saturating_mul(self.cell_w), self.y);
        }
    }

    fn erase_display_from_cursor(&mut self) {
        self.erase_line_range(self.x, self.columns());

        let current_row = self.y / self.cell_h;

        for row in current_row.saturating_add(1)..self.rows() {
            for column in 0..self.columns() {
                self.clear_cell(
                    column.saturating_mul(self.cell_w),
                    row.saturating_mul(self.cell_h),
                );
            }
        }
    }

    fn erase_display_to_cursor(&mut self) {
        let current_row = self.y / self.cell_h;

        for row in 0..current_row {
            for column in 0..self.columns() {
                self.clear_cell(
                    column.saturating_mul(self.cell_w),
                    row.saturating_mul(self.cell_h),
                );
            }
        }

        self.erase_line_range(0, self.x.saturating_add(1));
    }

    fn finish_csi(&mut self, command: u8) {
        match command {
            b'A' => {
                let amount = self.csi_param(0, 1);
                let row = self.y / self.cell_h;
                self.y = row.saturating_sub(amount).saturating_mul(self.cell_h);
            }

            b'B' => {
                let amount = self.csi_param(0, 1);
                let row = self.y / self.cell_h;
                self.y = row
                    .saturating_add(amount)
                    .min(self.rows().saturating_sub(1))
                    .saturating_mul(self.cell_h);
            }

            b'C' => {
                let amount = self.csi_param(0, 1);
                self.x = self
                    .x
                    .saturating_add(amount)
                    .min(self.columns().saturating_sub(1));
            }

            b'D' => {
                let amount = self.csi_param(0, 1);
                self.x = self.x.saturating_sub(amount);
            }

            b'E' => {
                let amount = self.csi_param(0, 1);
                let row = self.y / self.cell_h;
                self.y = row
                    .saturating_add(amount)
                    .min(self.rows().saturating_sub(1))
                    .saturating_mul(self.cell_h);
                self.x = 0;
            }

            b'F' => {
                let amount = self.csi_param(0, 1);
                let row = self.y / self.cell_h;
                self.y = row.saturating_sub(amount).saturating_mul(self.cell_h);
                self.x = 0;
            }

            b'G' | b'`' => {
                self.x = self
                    .csi_param(0, 1)
                    .saturating_sub(1)
                    .min(self.columns().saturating_sub(1));
            }

            b'd' => {
                let row = self.csi_param(0, 1);
                self.y = row
                    .saturating_sub(1)
                    .min(self.rows().saturating_sub(1))
                    .saturating_mul(self.cell_h);
            }

            b'H' | b'f' => {
                self.cursor_position(self.csi_param(0, 1), self.csi_param(1, 1));
            }

            b'J' => match self.csi_param(0, 0) {
                0 => self.erase_display_from_cursor(),
                1 => self.erase_display_to_cursor(),
                2 | 3 => self.clear(),
                _ => {}
            },

            b'K' => match self.csi_param(0, 0) {
                0 => self.erase_line_range(self.x, self.columns()),
                1 => self.erase_line_range(0, self.x.saturating_add(1)),
                2 => self.erase_line_range(0, self.columns()),
                _ => {}
            },

            b'P' => {
                let amount = self
                    .csi_param(0, 1)
                    .min(self.columns().saturating_sub(self.x));

                self.erase_line_range(self.x, self.x.saturating_add(amount));
            }

            b'@' => {}

            b'm' | b'h' | b'l' | b'n' | b'r' | b's' | b'u' => match command {
                b's' => {
                    self.saved_x = self.x;
                    self.saved_y = self.y;
                }

                b'u' => {
                    self.x = self.saved_x;
                    self.y = self.saved_y;
                }

                _ => {}
            },

            _ => {}
        }
    }

    pub fn clear(&mut self) {
        unsafe {
            core::ptr::write_bytes(self.buffer.as_mut_ptr(), 0, self.buffer.len());
        }

        self.x = 0;
        self.y = 0;
        self.ansi_state = 0;
        self.csi_params = [0; 8];
        self.csi_count = 0;
        self.csi_value = 0;
        self.csi_have_value = false;
        self.csi_private = false;
    }

    pub fn backspace(&mut self) {
        if self.mirror_serial {
            crate::console::serial_write_byte(0x08);
            crate::console::serial_write_byte(b' ');
            crate::console::serial_write_byte(0x08);
        }

        let columns = self.info.width / self.cell_w;

        if self.x > 0 {
            self.x -= 1;
        } else {
            if self.y < self.cell_h || columns == 0 {
                return;
            }

            self.y -= self.cell_h;
            self.x = columns - 1;
        }

        let px = self.x.saturating_mul(self.cell_w);

        let py = self.y;

        self.clear_cell(px, py);
    }

    pub fn write_char(&mut self, character: char) -> fmt::Result {
        if character.is_ascii() {
            self.put_char(character as u8);
        }

        Ok(())
    }

    #[inline(always)]
    fn clear_cell(&mut self, x: usize, y: usize) {
        if x >= self.info.width || y >= self.info.height || self.info.bytes_per_pixel == 0 {
            return;
        }

        let end_x = x.saturating_add(self.cell_w).min(self.info.width);
        let end_y = y.saturating_add(self.cell_h).min(self.info.height);
        let width = end_x - x;
        let bpp = self.info.bytes_per_pixel;
        let row_bytes = width.saturating_mul(bpp);
        let x_bytes = x.saturating_mul(bpp);

        for py in y..end_y {
            let offset = py
                .saturating_mul(self.info.stride)
                .saturating_mul(bpp)
                .saturating_add(x_bytes);
            let end = offset.saturating_add(row_bytes);

            if end > self.buffer.len() {
                break;
            }

            unsafe {
                core::ptr::write_bytes(self.buffer.as_mut_ptr().add(offset), 0, row_bytes);
            }
        }
    }

    fn newline(&mut self) {
        self.x = 0;

        self.y = self.y.saturating_add(self.cell_h);

        if self.y.saturating_add(self.font.height) >= self.info.height {
            self.scroll();
        }
    }

    fn scroll(&mut self) {
        let row_bytes = self.info.stride.saturating_mul(self.info.bytes_per_pixel);

        let bytes = self.cell_h.saturating_mul(row_bytes);

        if bytes >= self.buffer.len() {
            self.clear();
            return;
        }

        let remaining = self.buffer.len() - bytes;

        self.buffer.copy_within(bytes..bytes + remaining, 0);

        for byte in &mut self.buffer[remaining..] {
            *byte = 0;
        }

        self.y = self.y.saturating_sub(self.cell_h);
    }

    fn put_char(&mut self, character: u8) {
        if self.mirror_serial {
            match character {
                b'\n' => {
                    crate::console::serial_write_byte(b'\r');
                    crate::console::serial_write_byte(b'\n');
                }

                b'\r' => {
                    crate::console::serial_write_byte(b'\r');
                }

                b'\t' => {
                    crate::console::serial_write_byte(b'\t');
                }

                _ => {
                    crate::console::serial_write_byte(character);
                }
            }
        }

        match character {
            b'\n' => {
                self.newline();
                return;
            }

            b'\r' => {
                self.x = 0;
                return;
            }

            b'\t' => {
                self.x = ((self.x / 4) + 1) * 4;
                return;
            }

            _ => {}
        }

        if self
            .x
            .saturating_mul(self.cell_w)
            .saturating_add(self.font.width)
            >= self.info.width
        {
            self.newline();
        }

        if self.y.saturating_add(self.font.height) >= self.info.height {
            self.scroll();
        }

        let px = self.x.saturating_mul(self.cell_w);

        let py = self.y;

        self.draw_glyph(character, px, py);

        self.x = self.x.saturating_add(1);
    }

    #[inline(always)]
    fn draw_glyph(&mut self, character: u8, x: usize, y: usize) {
        let glyph = self.font.glyph(character);
        let row_bytes = self.font.row_bytes();

        if self.info.bytes_per_pixel < 3 || x >= self.info.width || y >= self.info.height {
            return;
        }

        let max_w = self.font.width.min(self.info.width - x);
        let max_h = self.font.height.min(self.info.height - y);
        let bpp = self.info.bytes_per_pixel;
        let (r, g, b) = FG;
        let gray = (r as u16 * 77 + g as u16 * 150 + b as u16 * 29) as u8;

        match self.info.pixel_format {
            PixelFormat::Rgb => {
                for row in 0..max_h {
                    let base = row.saturating_mul(row_bytes);
                    let offset = y
                        .saturating_add(row)
                        .saturating_mul(self.info.stride)
                        .saturating_mul(bpp)
                        .saturating_add(x.saturating_mul(bpp));
                    let end = offset.saturating_add(max_w.saturating_mul(bpp));

                    if end > self.buffer.len() {
                        break;
                    }

                    let dst = unsafe { self.buffer.as_mut_ptr().add(offset) };

                    for col in 0..max_w {
                        let index = base + col / 8;
                        if index < glyph.len() && glyph[index] & (0x80 >> (col & 7)) != 0 {
                            let p = unsafe { dst.add(col * bpp) };
                            unsafe {
                                *p = r;
                                *p.add(1) = g;
                                *p.add(2) = b;
                            }
                        }
                    }
                }
            }

            PixelFormat::Bgr => {
                for row in 0..max_h {
                    let base = row.saturating_mul(row_bytes);
                    let offset = y
                        .saturating_add(row)
                        .saturating_mul(self.info.stride)
                        .saturating_mul(bpp)
                        .saturating_add(x.saturating_mul(bpp));
                    let end = offset.saturating_add(max_w.saturating_mul(bpp));

                    if end > self.buffer.len() {
                        break;
                    }

                    let dst = unsafe { self.buffer.as_mut_ptr().add(offset) };

                    for col in 0..max_w {
                        let index = base + col / 8;
                        if index < glyph.len() && glyph[index] & (0x80 >> (col & 7)) != 0 {
                            let p = unsafe { dst.add(col * bpp) };
                            unsafe {
                                *p = b;
                                *p.add(1) = g;
                                *p.add(2) = r;
                            }
                        }
                    }
                }
            }

            PixelFormat::U8 => {
                for row in 0..max_h {
                    let base = row.saturating_mul(row_bytes);
                    let offset = y
                        .saturating_add(row)
                        .saturating_mul(self.info.stride)
                        .saturating_mul(bpp)
                        .saturating_add(x.saturating_mul(bpp));
                    let end = offset.saturating_add(max_w.saturating_mul(bpp));

                    if end > self.buffer.len() {
                        break;
                    }

                    let dst = unsafe { self.buffer.as_mut_ptr().add(offset) };

                    for col in 0..max_w {
                        let index = base + col / 8;
                        if index < glyph.len() && glyph[index] & (0x80 >> (col & 7)) != 0 {
                            let p = unsafe { dst.add(col * bpp) };
                            unsafe {
                                *p = gray;
                            }
                        }
                    }
                }
            }

            _ => {}
        }
    }
}

impl Write for FrameTerminal<'_> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for byte in s.bytes() {
            self.put_char(byte);
        }

        Ok(())
    }
}

fn read_u32(data: &[u8], offset: usize) -> u32 {
    (data[offset] as u32)
        | ((data[offset + 1] as u32) << 8)
        | ((data[offset + 2] as u32) << 16)
        | ((data[offset + 3] as u32) << 24)
}
