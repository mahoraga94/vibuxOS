use bootloader_api::info::{
    FrameBuffer,
    FrameBufferInfo,
    PixelFormat,
};

const BG: (u8, u8, u8) = (8, 10, 8);
const FG: (u8, u8, u8) = (180, 230, 150);
const MUTED: (u8, u8, u8) = (105, 130, 95);
const PANEL: (u8, u8, u8) = (16, 19, 16);
const BAR_BG: (u8, u8, u8) = (25, 30, 25);

pub fn render(
    framebuffer: &mut FrameBuffer,
    usable_bytes: u64,
    reported_bytes: u64,
    architecture: &str,
) {
    let info = framebuffer.info();
    let buffer = framebuffer.buffer_mut();

    clear(buffer, info, BG);

    let scale = calculate_scale(info);

    let margin_x = (info.width / 12).max(24);
    let margin_y = (info.height / 12).max(20);

    let content_width =
        info.width
            .saturating_sub(margin_x.saturating_mul(2));

    let accent_height =
        (info.height / 180).clamp(2, 8);

    fill_rect(
        buffer,
        info,
        0,
        0,
        info.width,
        accent_height,
        FG,
    );

    draw_centered(
        buffer,
        info,
        b"VIBUXOS",
        margin_y,
        scale,
        FG,
    );

    let title_height =
        7usize.saturating_mul(scale);

    let subtitle_y =
        margin_y
            .saturating_add(title_height)
            .saturating_add(scale.saturating_mul(2));

    draw_centered(
        buffer,
        info,
        b"KERNEL ONLINE",
        subtitle_y,
        scale.saturating_sub(1).max(2),
        MUTED,
    );

    let panel_width =
        content_width;

    let panel_height =
        (info.height / 2).clamp(220, info.height.saturating_sub(120));

    let panel_x =
        info.width
            .saturating_sub(panel_width)
            / 2;

    let panel_y =
        info.height
            .saturating_sub(panel_height)
            / 2;

    draw_panel(
        buffer,
        info,
        panel_x,
        panel_y,
        panel_width,
        panel_height,
    );

    let center_x =
        panel_x + panel_width / 2;

    let status_scale =
        scale.saturating_sub(1).max(2);

    let status_y =
        panel_y
            + panel_height / 10;

    draw_centered_at(
        buffer,
        info,
        b"MEMORY ONLINE",
        center_x,
        status_y,
        status_scale,
        FG,
    );

    let ready_y =
        panel_y
            + panel_height * 3 / 10;

    draw_centered_at(
        buffer,
        info,
        b"KERNEL READY",
        center_x,
        ready_y,
        status_scale,
        FG,
    );

    let arch_y =
        panel_y
            + panel_height * 5 / 10;

    draw_centered_at(
        buffer,
        info,
        architecture.as_bytes(),
        center_x,
        arch_y,
        status_scale,
        MUTED,
    );

    let pad =
        (panel_width / 14).max(20);

    let bar_width =
        panel_width
            .saturating_sub(pad.saturating_mul(2));

    let bar_height =
        (info.height / 90).clamp(8, 20);

    let bar_x =
        panel_x + pad;

    let bar_y =
        panel_y + panel_height * 7 / 10;

    fill_rect(
        buffer,
        info,
        bar_x,
        bar_y,
        bar_width,
        bar_height,
        BAR_BG,
    );

    if reported_bytes > 0 {
        let filled =
            (
                (bar_width as u128)
                    .saturating_mul(
                        usable_bytes as u128
                    )
                    / reported_bytes as u128
            ) as usize;

        fill_rect(
            buffer,
            info,
            bar_x,
            bar_y,
            filled.min(bar_width),
            bar_height,
            FG,
        );
    }

    let footer_scale =
        scale.saturating_sub(2).max(2);

    let footer_y =
        panel_y
            .saturating_add(panel_height)
            .saturating_add(
                footer_scale.saturating_mul(3)
            );

    if footer_y
        .saturating_add(
            footer_scale.saturating_mul(7)
        )
        < info.height
    {
        draw_centered(
            buffer,
            info,
            b"VIBUX KERNEL",
            footer_y,
            footer_scale,
            MUTED,
        );
    }
}

fn calculate_scale(
    info: FrameBufferInfo,
) -> usize {
    let dimension =
        info.width.min(info.height);

    match dimension {
        0..=399 => 2,
        400..=599 => 3,
        600..=899 => 4,
        900..=1199 => 5,
        1200..=1599 => 6,
        _ => 7,
    }
}

fn draw_centered(
    buffer: &mut [u8],
    info: FrameBufferInfo,
    text: &[u8],
    y: usize,
    scale: usize,
    color: (u8, u8, u8),
) {
    let width =
        text_width(text, scale);

    let x =
        info.width.saturating_sub(width) / 2;

    draw_text(
        buffer,
        info,
        text,
        x,
        y,
        scale,
        color,
    );
}

fn draw_centered_at(
    buffer: &mut [u8],
    info: FrameBufferInfo,
    text: &[u8],
    center_x: usize,
    y: usize,
    scale: usize,
    color: (u8, u8, u8),
) {
    let width =
        text_width(text, scale);

    let x =
        center_x.saturating_sub(width / 2);

    draw_text(
        buffer,
        info,
        text,
        x,
        y,
        scale,
        color,
    );
}

fn text_width(
    text: &[u8],
    scale: usize,
) -> usize {
    if text.is_empty() {
        return 0;
    }

    text.len()
        .saturating_mul(
            6usize.saturating_mul(scale)
        )
        .saturating_sub(scale)
}

fn draw_text(
    buffer: &mut [u8],
    info: FrameBufferInfo,
    text: &[u8],
    mut x: usize,
    y: usize,
    scale: usize,
    color: (u8, u8, u8),
) {
    for &c in text {
        draw_char(
            buffer,
            info,
            c,
            x,
            y,
            scale,
            color,
        );

        x = x.saturating_add(
            6usize.saturating_mul(scale)
        );
    }
}

fn draw_char(
    buffer: &mut [u8],
    info: FrameBufferInfo,
    c: u8,
    x: usize,
    y: usize,
    scale: usize,
    color: (u8, u8, u8),
) {
    let rows = glyph(c);
    let mut row = 0usize;

    while row < 7 {
        let bits = rows[row];
        let mut col = 0usize;

        while col < 5 {
            if bits & (1 << (4 - col)) != 0 {
                let start = col;
                col += 1;

                while col < 5 && bits & (1 << (4 - col)) != 0 {
                    col += 1;
                }

                fill_rect(
                    buffer,
                    info,
                    x + start * scale,
                    y + row * scale,
                    (col - start) * scale,
                    scale,
                    color,
                );
            } else {
                col += 1;
            }
        }

        row += 1;
    }
}

fn draw_panel(/
    buffer: &mut [u8],
    info: FrameBufferInfo,
    x: usize,
    y: usize,
    width: usize,
    height: usize,
) {
    let border =
        (info.width.min(info.height) / 500)
            .clamp(1, 3);

    fill_rect(
        buffer,
        info,
        x,
        y,
        width,
        height,
        PANEL,
    );

    fill_rect(
        buffer,
        info,
        x,
        y,
        width,
        border,
        MUTED,
    );

    fill_rect(
        buffer,
        info,
        x,
        y + height - border,
        width,
        border,
        MUTED,
    );

    fill_rect(
        buffer,
        info,
        x,
        y,
        border,
        height,
        MUTED,
    );

    fill_rect(
        buffer,
        info,
        x + width - border,
        y,
        border,
        height,
        MUTED,
    );
}

#[inline(always)]
fn clear(
    buffer: &mut [u8],
    info: FrameBufferInfo,
    color: (u8, u8, u8),
) {
    fill_rect(
        buffer,
        info,
        0,
        0,
        info.width,
        info.height,
        color,
    );
}

#[inline(always)]
#[inline(always)]
fn fill_rect(
    buffer: &mut [u8],
    info: FrameBufferInfo,
    x: usize,
    y: usize,
    width: usize,
    height: usize,
    color: (u8, u8, u8),
) {
    let bpp = info.bytes_per_pixel;

    if bpp < 3 || x >= info.width || y >= info.height || width == 0 || height == 0 {
        return;
    }

    let x_end = x.saturating_add(width).min(info.width);
    let y_end = y.saturating_add(height).min(info.height);
    let row_width = x_end - x;
    let row_bytes = row_width.saturating_mul(bpp);
    let x_bytes = x.saturating_mul(bpp);
    let (r, g, b) = color;

    match info.pixel_format {
        PixelFormat::Rgb => {
            for yy in y..y_end {
                let row = yy
                    .saturating_mul(info.stride)
                    .saturating_mul(bpp)
                    .saturating_add(x_bytes);
                let end = row.saturating_add(row_bytes);

                if end > buffer.len() {
                    break;
                }

                let dst = unsafe { buffer.as_mut_ptr().add(row) };
                let mut p = 0usize;

                while p < row_width {
                    let q = unsafe { dst.add(p * bpp) };
                    unsafe {
                        *q = r;
                        *q.add(1) = g;
                        *q.add(2) = b;
                        if bpp >= 4 {
                            *q.add(3) = 255;
                        }
                    }
                    p += 1;
                }
            }
        }

        PixelFormat::Bgr => {
            for yy in y..y_end {
                let row = yy
                    .saturating_mul(info.stride)
                    .saturating_mul(bpp)
                    .saturating_add(x_bytes);
                let end = row.saturating_add(row_bytes);

                if end > buffer.len() {
                    break;
                }

                let dst = unsafe { buffer.as_mut_ptr().add(row) };
                let mut p = 0usize;

                while p < row_width {
                    let q = unsafe { dst.add(p * bpp) };
                    unsafe {
                        *q = b;
                        *q.add(1) = g;
                        *q.add(2) = r;
                        if bpp >= 4 {
                            *q.add(3) = 255;
                        }
                    }
                    p += 1;
                }
            }
        }

        PixelFormat::U8 => {
            let gray = (r as u16 * 77 + g as u16 * 150 + b as u16 * 29) as u8;

            for yy in y..y_end {
                let row = yy
                    .saturating_mul(info.stride)
                    .saturating_mul(bpp)
                    .saturating_add(x_bytes);
                let end = row.saturating_add(row_bytes);

                if end > buffer.len() {
                    break;
                }

                let dst = unsafe { buffer.as_mut_ptr().add(row) };
                let mut p = 0usize;

                while p < row_width {
                    let q = unsafe { dst.add(p * bpp) };
                    unsafe {
                        *q = gray;
                        if bpp >= 4 {
                            *q.add(3) = 255;
                        }
                    }
                    p += 1;
                }
            }
        }

        _ => {}
    }
}

fn glyph(
    c: u8,
) -> [u8; 7] {
    match c {
        b'A' => [
            0b01110,
            0b10001,
            0b10001,
            0b11111,
            0b10001,
            0b10001,
            0b10001,
        ],

        b'B' => [
            0b11110,
            0b10001,
            0b10001,
            0b11110,
            0b10001,
            0b10001,
            0b11110,
        ],

        b'C' => [
            0b01111,
            0b10000,
            0b10000,
            0b10000,
            0b10000,
            0b10000,
            0b01111,
        ],

        b'D' => [
            0b11110,
            0b10001,
            0b10001,
            0b10001,
            0b10001,
            0b10001,
            0b11110,
        ],

        b'E' => [
            0b11111,
            0b10000,
            0b10000,
            0b11110,
            0b10000,
            0b10000,
            0b11111,
        ],

        b'F' => [
            0b11111,
            0b10000,
            0b10000,
            0b11110,
            0b10000,
            0b10000,
            0b10000,
        ],

        b'G' => [
            0b01111,
            0b10000,
            0b10000,
            0b10111,
            0b10001,
            0b10001,
            0b01111,
        ],

        b'H' => [
            0b10001,
            0b10001,
            0b10001,
            0b11111,
            0b10001,
            0b10001,
            0b10001,
        ],

        b'I' => [
            0b11111,
            0b00100,
            0b00100,
            0b00100,
            0b00100,
            0b00100,
            0b11111,
        ],

        b'J' => [
            0b00111,
            0b00010,
            0b00010,
            0b00010,
            0b00010,
            0b10010,
            0b01100,
        ],

        b'K' => [
            0b10001,
            0b10010,
            0b10100,
            0b11000,
            0b10100,
            0b10010,
            0b10001,
        ],

        b'L' => [
            0b10000,
            0b10000,
            0b10000,
            0b10000,
            0b10000,
            0b10000,
            0b11111,
        ],

        b'M' => [
            0b10001,
            0b11011,
            0b10101,
            0b10101,
            0b10001,
            0b10001,
            0b10001,
        ],

        b'N' => [
            0b10001,
            0b11001,
            0b10101,
            0b10011,
            0b10001,
            0b10001,
            0b10001,
        ],

        b'O' => [
            0b01110,
            0b10001,
            0b10001,
            0b10001,
            0b10001,
            0b10001,
            0b01110,
        ],

        b'P' => [
            0b11110,
            0b10001,
            0b10001,
            0b11110,
            0b10000,
            0b10000,
            0b10000,
        ],

        b'Q' => [
            0b01110,
            0b10001,
            0b10001,
            0b10001,
            0b10101,
            0b10010,
            0b01101,
        ],

        b'R' => [
            0b11110,
            0b10001,
            0b10001,
            0b11110,
            0b10100,
            0b10010,
            0b10001,
        ],

        b'S' => [
            0b01111,
            0b10000,
            0b10000,
            0b01110,
            0b00001,
            0b00001,
            0b11110,
        ],

        b'T' => [
            0b11111,
            0b00100,
            0b00100,
            0b00100,
            0b00100,
            0b00100,
            0b00100,
        ],

        b'U' => [
            0b10001,
            0b10001,
            0b10001,
            0b10001,
            0b10001,
            0b10001,
            0b01110,
        ],

        b'V' => [
            0b10001,
            0b10001,
            0b10001,
            0b10001,
            0b01010,
            0b01010,
            0b00100,
        ],

        b'W' => [
            0b10001,
            0b10001,
            0b10101,
            0b10101,
            0b10101,
            0b11011,
            0b10001,
        ],

        b'X' => [
            0b10001,
            0b01010,
            0b00100,
            0b00100,
            0b00100,
            0b01010,
            0b10001,
        ],

        b'Y' => [
            0b10001,
            0b10001,
            0b01010,
            0b00100,
            0b00100,
            0b00100,
            0b00100,
        ],

        b'Z' => [
            0b11111,
            0b00001,
            0b00010,
            0b00100,
            0b01000,
            0b10000,
            0b11111,
        ],

        b'0' => [
            0b01110,
            0b10001,
            0b10011,
            0b10101,
            0b11001,
            0b10001,
            0b01110,
        ],

        b'1' => [
            0b00100,
            0b01100,
            0b00100,
            0b00100,
            0b00100,
            0b00100,
            0b01110,
        ],

        b'2' => [
            0b01110,
            0b10001,
            0b00001,
            0b00010,
            0b00100,
            0b01000,
            0b11111,
        ],

        b'3' => [
            0b11110,
            0b00001,
            0b00001,
            0b01110,
            0b00001,
            0b00001,
            0b11110,
        ],

        b'4' => [
            0b00010,
            0b00110,
            0b01010,
            0b10010,
            0b11111,
            0b00010,
            0b00010,
        ],

        b'5' => [
            0b11111,
            0b10000,
            0b10000,
            0b11110,
            0b00001,
            0b00001,
            0b11110,
        ],

        b'6' => [
            0b01110,
            0b10000,
            0b10000,
            0b11110,
            0b10001,
            0b10001,
            0b01110,
        ],

        b'7' => [
            0b11111,
            0b00001,
            0b00010,
            0b00100,
            0b01000,
            0b01000,
            0b01000,
        ],

        b'8' => [
            0b01110,
            0b10001,
            0b10001,
            0b01110,
            0b10001,
            0b10001,
            0b01110,
        ],

        b'9' => [
            0b01110,
            0b10001,
            0b10001,
            0b01111,
            0b00001,
            0b00001,
            0b01110,
        ],

        b'-' => [
            0b00000,
            0b00000,
            0b00000,
            0b11111,
            0b00000,
            0b00000,
            0b00000,
        ],

        b' ' => [0; 7],

        _ => [0; 7],
    }
}
