/// Pool de buffers para reutilizar y evitar allocaciones costosas durante el renderizado.
#[allow(dead_code)]
pub struct BufferPool {
    buffers: Vec<Vec<u8>>,
}

#[allow(dead_code)]
impl BufferPool {
    pub fn new() -> Self {
        Self {
            buffers: Vec::new(),
        }
    }

    pub fn acquire(&mut self, capacity: usize) -> Vec<u8> {
        if let Some(mut buf) = self.buffers.pop() {
            buf.clear();
            buf.reserve(capacity.saturating_sub(buf.capacity()));
            buf
        } else {
            Vec::with_capacity(capacity)
        }
    }

    pub fn release(&mut self, buf: Vec<u8>) {
        if self.buffers.len() < 8 {
            self.buffers.push(buf);
        }
    }
}

impl Default for BufferPool {
    fn default() -> Self {
        Self::new()
    }
}
