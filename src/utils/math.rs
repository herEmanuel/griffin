pub fn div_ceil(x: usize, y: usize) -> usize {
    (x + y - 1) / y
}

pub fn round_up(number: usize, multiple: usize) -> usize {
    ((number + multiple - 1) / multiple) * multiple
}
