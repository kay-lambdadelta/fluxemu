pub struct PowerOfTwoIter<const MAX: usize> {
    remaining: usize,
}

impl<const MAX: usize> PowerOfTwoIter<MAX> {
    pub fn new(start: usize) -> Self {
        assert!(MAX.is_power_of_two());

        Self { remaining: start }
    }
}

impl<const MAX: usize> Iterator for PowerOfTwoIter<MAX> {
    type Item = usize;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }

        let chunk = if self.remaining >= MAX {
            MAX
        } else {
            1usize << self.remaining.ilog2()
        };

        self.remaining -= chunk;
        Some(chunk)
    }
}
