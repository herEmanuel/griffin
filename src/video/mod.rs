use stivale_boot::v2::StivaleFramebufferTag;

mod fonts;

pub struct Video {
    cursor_x: usize,
    cursor_y: usize,
    fb_addr: *mut u32,
    height: u16,
    width: u16,
    pitch: u16,
    font: fonts::Font,
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
            font: fonts::Font::new(),
        }
    }

    pub fn putc(&mut self, character: char, color: u32) {
        match character {
            '\n' => {
                self.cursor_y += self.font.height as usize + 2;
                self.cursor_x = 10;
                return;
            }

            _ => {}
        }

        let index = character as u32 * self.font.height;
        for col in 0..self.font.height {
            for row in 0..self.font.width {
                if (self.font.bitmap[(index + col) as usize] >> (7 - row)) & 1 == 1 {
                    let offset = self.cursor_x
                        + row as usize
                        + (self.cursor_y + col as usize) * self.pitch as usize / 4;

                    unsafe {
                        (*self.fb_addr.offset(offset as isize)) = color;
                    }
                }
            }
        }

        let char_width = self.font.width as usize + 2;
        self.cursor_x += char_width;
        if self.cursor_x + char_width >= self.width as usize {
            self.cursor_x = 10;
            self.cursor_y += self.font.height as usize + 2;
        }
    }

    pub fn print(&mut self, msg: &str) {
        for c in msg.chars() {
            self.putc(c, 0xffffff);
        }
    }
}
