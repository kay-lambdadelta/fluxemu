pub struct PowerOfTwoIter {
    remaining: usize,
    max: usize,
}

impl PowerOfTwoIter {
    pub fn new(start: usize, max: usize) -> Self {
        debug_assert!(max.is_power_of_two());

        Self {
            remaining: start,
            max,
        }
    }
}

impl Iterator for PowerOfTwoIter {
    type Item = usize;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }

        let chunk = if self.remaining >= self.max {
            self.max
        } else {
            1usize << self.remaining.ilog2()
        };

        self.remaining -= chunk;
        Some(chunk)
    }
}
