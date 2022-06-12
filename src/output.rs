const CHUNK_SIZE: usize = 1024usize;

type Chunk = Box<std::mem::MaybeUninit<[u8; CHUNK_SIZE]>>;

/// Manages Output of a process. Allows interleaved iterating and appending
#[allow(unused)] // TODO: Remove
#[derive(Default)]
pub struct Output {
    chunks: Vec<Chunk>,
    last_chunk_size: usize,
}

#[derive(Copy, Clone, Default, PartialEq, Eq)]
pub struct OutputPos(usize);

impl OutputPos {
    pub fn offset(self, offset: usize) -> Self {
        OutputPos(self.0 + offset)
    }

    fn chunk_index(self) -> usize {
        self.0 / CHUNK_SIZE
    }

    fn pos_in_chunk(self) -> usize {
        self.0 % CHUNK_SIZE
    }
}

impl Output {
    pub fn len(&self) -> usize {
        if self.chunks.is_empty() {
            return 0;
        }
        (self.chunks.len() - 1) * CHUNK_SIZE + self.last_chunk_size
    }

    pub fn append(&mut self, data: &'_ [u8]) {
        let mut data = data;

        if let Some(last_chunk) = self.chunks.last_mut() {
            let size = data.len().min(CHUNK_SIZE - self.last_chunk_size);
            if size > 0 {
                let ptr = last_chunk.as_mut_ptr() as *mut u8;
                unsafe {
                    let ptr = ptr.offset(self.last_chunk_size as isize);

                    // Safety: There is no legal way for the caller to obtain ```data```
                    // in an overlapping way. Although the current chunk may be already
                    // exposed, it is only possible to aquire a reference to the first
                    // part of the chunk. We only modify the uninitialized part
                    std::ptr::copy_nonoverlapping(data.as_ptr(), ptr, size);
                }
                data = &data[size..];
                self.last_chunk_size += size;
            }
        }

        while !data.is_empty() {
            let size = data.len().min(CHUNK_SIZE);
            let mut chunk: Chunk = Box::new(std::mem::MaybeUninit::uninit());
            let ptr = chunk.as_mut_ptr() as *mut u8;
            unsafe {
                std::ptr::copy_nonoverlapping(data.as_ptr(), ptr, size);
            }

            self.chunks.push(chunk);
            self.last_chunk_size = size;

            data = &data[size..];
        }
    }

    pub fn try_read<'a>(&'a self, pos: OutputPos) -> Option<(OutputPos, &'a [u8])> {
        let length = self.len();

        if pos.0 >= length {
            return None;
        }

        let boundary = pos.0 + CHUNK_SIZE;
        let boundary = boundary - boundary % CHUNK_SIZE;
        let boundary = boundary.min(length);

        let chunk_idx = pos.chunk_index();
        let pos_in_chunk = pos.pos_in_chunk();

        {
            let size = boundary - pos.0;

            let chunk = self.chunks.get(chunk_idx).unwrap();

            let ptr = chunk.as_ptr() as *const u8;
            let data = unsafe {
                let ptr = ptr.offset(pos_in_chunk as isize);
                std::slice::from_raw_parts::<'a>(ptr, size)
            };

            return Some((pos.offset(size), data));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::CHUNK_SIZE;

    fn collect(output: &'_ super::Output) -> Vec<u8> {
        let mut result = Vec::new();
        let mut pos = super::OutputPos::default();
        while let Some((next_pos, chunk)) = output.try_read(pos) {
            pos = next_pos;
            result.extend_from_slice(chunk);
        }

        result
    }

    #[test]
    fn append_small() {
        let mut output = super::Output::default();
        output.append(b"Hello ");
        output.append(b"world");
        output.append(b"!");

        assert_eq!(output.chunks.len(), 1);
        assert_eq!(output.last_chunk_size, 12);
        assert_eq!(collect(&output), b"Hello world!");
    }

    #[test]
    fn append_huge() {
        let mut output = super::Output::default();
        let data = [0u8; 4 * CHUNK_SIZE + 10];

        output.append(&data);

        assert_eq!(output.chunks.len(), 5);
        assert_eq!(output.last_chunk_size, 10);
    }

    #[test]
    fn append_iterate_interleaved() {
        let mut output = super::Output::default();
        assert_eq!(collect(&output), b"");

        output.append(b"Hello");
        assert_eq!(collect(&output), b"Hello");

        output.append(b" world");
        assert_eq!(collect(&output), b"Hello world");
    }
}
