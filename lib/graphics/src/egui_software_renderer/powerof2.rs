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

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }

        if self.remaining >= MAX {
            self.remaining -= MAX;

            Some(MAX)
        } else {
            let highest = 1usize << (usize::BITS - 1 - self.remaining.leading_zeros());
            self.remaining -= highest;

            Some(highest)
        }
    }
}
