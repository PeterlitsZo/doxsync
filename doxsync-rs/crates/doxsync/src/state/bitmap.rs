pub(super) struct Bitmap {
    data: Vec<u64>,
    l1: Vec<u64>,
    size: usize,
}

impl Bitmap {
    const BITS_PER_WORD: usize = u64::BITS as usize;
    const L1_CHILDREN: usize = 128;
    const L1_SLOTS_PER_WORD: usize = Self::BITS_PER_WORD / 2;

    pub(super) fn new(size: usize) -> Self {
        assert_eq!(
            size % Self::L1_CHILDREN,
            0,
            "bitmap size must be a multiple of L1_CHILDREN"
        );

        let data_len = size / Self::BITS_PER_WORD;
        let l1_len = (size / Self::L1_CHILDREN).div_ceil(Self::L1_SLOTS_PER_WORD);
        Self {
            data: vec![0; data_len],
            l1: vec![0; l1_len],
            size,
        }
    }

    pub(super) fn alloc(&mut self) -> Option<usize> {
        let l1_len = self.size / Self::L1_CHILDREN;
        for l1_idx in 0..l1_len {
            let l1_word_idx = l1_idx / Self::L1_SLOTS_PER_WORD;
            let l1_shift = (l1_idx % Self::L1_SLOTS_PER_WORD) * 2;
            let state = (self.l1[l1_word_idx] >> l1_shift) & 0b11;

            match state {
                0b00 => {
                    let data_idx = l1_idx * Self::L1_CHILDREN;
                    self.data[data_idx / Self::BITS_PER_WORD] |=
                        1 << (data_idx % Self::BITS_PER_WORD);
                    self.l1[l1_word_idx] |= 0b01 << l1_shift;
                    return Some(data_idx);
                }
                0b01 => {
                    let data_word_begin = l1_idx * Self::L1_CHILDREN / Self::BITS_PER_WORD;
                    let data_word_end = (data_word_begin + Self::L1_CHILDREN / Self::BITS_PER_WORD)
                        .min(self.data.len());

                    let mut result = None;
                    for data_word_idx in data_word_begin..data_word_end {
                        let free_bits = !self.data[data_word_idx];
                        if free_bits == 0 {
                            continue;
                        }

                        let bit_idx = free_bits.trailing_zeros() as usize;
                        self.data[data_word_idx] |= 1 << bit_idx;
                        result = Some(data_word_idx * Self::BITS_PER_WORD + bit_idx);
                        break;
                    }
                    let result = result.expect("a partial L1 range must contain a free bit");

                    if self.l1_range_is_full(l1_idx) {
                        self.l1[l1_word_idx] |= 0b10 << l1_shift;
                    }

                    return Some(result);
                }
                0b11 => continue,
                _ => unreachable!(),
            }
        }
        None
    }

    pub(super) fn alloc_at(&mut self, index: usize) {
        assert!(index < self.size, "index must be within the bitmap");

        let data_word = self
            .data
            .get_mut(index / Self::BITS_PER_WORD)
            .expect("index must be within the bitmap");
        let data_mask = 1 << (index % Self::BITS_PER_WORD);
        assert!(*data_word & data_mask == 0, "index must be unallocated");
        *data_word |= data_mask;

        let l1_idx = index / Self::L1_CHILDREN;
        let l1_shift = (l1_idx % Self::L1_SLOTS_PER_WORD) * 2;
        let l1_mask = 0b11 << l1_shift;

        let state = if self.l1_range_is_full(l1_idx) {
            0b11
        } else {
            0b01
        };

        let l1_word = &mut self.l1[l1_idx / Self::L1_SLOTS_PER_WORD];
        *l1_word = (*l1_word & !l1_mask) | (state << l1_shift);
    }

    pub(super) fn dealloc(&mut self, index: usize) {
        assert!(index < self.size, "index must be within the bitmap");

        let data_word = self
            .data
            .get_mut(index / Self::BITS_PER_WORD)
            .expect("index must be within the bitmap");
        let data_mask = 1 << (index % Self::BITS_PER_WORD);
        assert!(*data_word & data_mask != 0, "index must be allocated");
        *data_word &= !data_mask;

        let l1_idx = index / Self::L1_CHILDREN;
        let l1_shift = (l1_idx % Self::L1_SLOTS_PER_WORD) * 2;
        let l1_mask = 0b11 << l1_shift;

        let state = if self.l1_range_is_empty(l1_idx) {
            0b00
        } else {
            0b01
        };

        let l1_word = &mut self.l1[l1_idx / Self::L1_SLOTS_PER_WORD];
        *l1_word = (*l1_word & !l1_mask) | (state << l1_shift);
    }

    fn l1_range_is_empty(&self, l1_idx: usize) -> bool {
        let data_word_begin = l1_idx * Self::L1_CHILDREN / Self::BITS_PER_WORD;
        let data_word_end =
            (data_word_begin + Self::L1_CHILDREN / Self::BITS_PER_WORD).min(self.data.len());
        self.data[data_word_begin..data_word_end]
            .iter()
            .all(|word| *word == 0)
    }

    fn l1_range_is_full(&self, l1_idx: usize) -> bool {
        let data_word_begin = l1_idx * Self::L1_CHILDREN / Self::BITS_PER_WORD;
        let data_word_end =
            (data_word_begin + Self::L1_CHILDREN / Self::BITS_PER_WORD).min(self.data.len());
        self.data[data_word_begin..data_word_end]
            .iter()
            .all(|word| *word == u64::MAX)
    }
}

#[cfg(test)]
mod tests {
    use crate::state::Bitmap;

    #[test]
    fn test_bitmap() {
        let mut bitmap = Bitmap::new(1024);

        // Simple allocate and deallocate.
        assert_eq!(bitmap.alloc(), Some(0));
        bitmap.dealloc(0);
        assert_eq!(bitmap.alloc(), Some(0));
        assert_eq!(bitmap.alloc(), Some(1));
        bitmap.dealloc(0);
        assert_eq!(bitmap.alloc(), Some(0));
        bitmap.dealloc(0);
        bitmap.dealloc(1);

        // Allocate 512 times.
        for i in 0..512 {
            assert_eq!(bitmap.alloc(), Some(i));
        }
        bitmap.dealloc(42);
        bitmap.dealloc(16);
        bitmap.dealloc(100);
        assert_eq!(bitmap.alloc(), Some(16));
        assert_eq!(bitmap.alloc(), Some(42));
        assert_eq!(bitmap.alloc(), Some(100));
    }
}
