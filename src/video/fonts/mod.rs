#[repr(C, packed)]
struct PsfHeader {
    magic: u32,
    version: u32,
    hdr_size: u32,
    flags: u32,
    glyph_count: u32,
    glyph_size: u32,
    height: u32,
    width: u32,
}

const PSF_MAGIC: u32 = 0x864ab572;

pub struct Font {
    pub bitmap: &'static [u8],
    pub height: u32,
    pub width: u32,
}

impl Font {
    pub fn new() -> Self {
        let bytes = include_bytes!("terminus.psf");
        let header;

        unsafe {
            header = &*(bytes as *const u8 as *const PsfHeader);
        }

        assert!(header.magic == PSF_MAGIC);

        Font {
            bitmap: &bytes[header.hdr_size as usize..],
            height: header.height,
            width: header.width,
        }
    }
}
