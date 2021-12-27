pub struct Bitmap<const SIZE: usize> {
    pub data: [u8; SIZE],
}

impl<const SIZE: usize> Bitmap<SIZE> {
    pub fn set_bit(&mut self, bit: usize) {
        self.data[bit / 8] |= 1 << (7 - bit % 8);
    }

    pub fn clear_bit(&mut self, bit: usize) {
        self.data[bit / 8] &= !(1 << (7 - bit % 8));
    }

    pub fn bit_at(&self, bit: usize) -> u8 {
        self.data[bit / 8] & (1 << (7 - bit % 8))
    }
}
