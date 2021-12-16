use stivale_boot::v2::StivaleFramebufferTag;

mod font;

pub struct Video {
    cursor_x: usize,
    cursor_y: usize,
    fb_addr: *mut u32,
    height: u16,
    width: u16,
    pitch: u16,
}

impl Video {
    pub fn new(fb_tag: &StivaleFramebufferTag) -> Self {
        Video {
            cursor_x: 10,
            cursor_y: 10,
            fb_addr: fb_tag.framebuffer_addr as *mut u32,
            height: fb_tag.framebuffer_height,
            width: fb_tag.framebuffer_width,
            pitch: fb_tag.framebuffer_pitch,
        }
    }

    pub fn putc(&mut self, character: char, color: u32) {
        match character {
            '\n' => {
                self.cursor_y += font::CHAR_HEIGHT as usize + 2;
                self.cursor_x = 0;
                return;
            }

            _ => {}
        }

        let index = character as u32 * font::CHAR_HEIGHT;
        for col in 0..font::CHAR_HEIGHT {
            for row in 0..font::CHAR_WIDTH {
                if (font::FONT[(index + col) as usize] >> (7 - row)) & 1 == 1 {
                    let offset = self.cursor_x
                        + row as usize
                        + (self.cursor_y + col as usize) * self.pitch as usize / 4;

                    unsafe {
                        (*self.fb_addr.offset(offset as isize)) = color;
                    }
                }
            }
        }

        self.cursor_x += font::CHAR_WIDTH as usize + 2;
        if self.cursor_x >= self.width as usize {
            self.cursor_x = 0;
            self.cursor_y += font::CHAR_HEIGHT as usize + 2;
        }
    }

    pub fn print(&mut self, msg: &str) {
        for c in msg.chars() {
            self.putc(c, 0xffffff);
        }
    }
}
