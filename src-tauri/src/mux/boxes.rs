//! Minimal ISO-BMFF box writer.

pub struct BoxWriter {
    pub buf: Vec<u8>,
    stack: Vec<usize>,
}

impl Default for BoxWriter {
    fn default() -> Self {
        Self::new()
    }
}

impl BoxWriter {
    pub fn new() -> Self {
        Self { buf: Vec::with_capacity(4096), stack: Vec::new() }
    }

    pub fn begin(&mut self, fourcc: &[u8; 4]) {
        self.stack.push(self.buf.len());
        self.u32(0);
        self.buf.extend_from_slice(fourcc);
    }

    /// Full box: version + 24-bit flags.
    pub fn begin_full(&mut self, fourcc: &[u8; 4], version: u8, flags: u32) {
        self.begin(fourcc);
        self.u32(((version as u32) << 24) | (flags & 0x00ff_ffff));
    }

    pub fn end(&mut self) {
        let start = self.stack.pop().expect("unbalanced box");
        let size = (self.buf.len() - start) as u32;
        self.buf[start..start + 4].copy_from_slice(&size.to_be_bytes());
    }

    pub fn pos(&self) -> usize {
        self.buf.len()
    }

    pub fn u8(&mut self, v: u8) {
        self.buf.push(v);
    }
    pub fn u16(&mut self, v: u16) {
        self.buf.extend_from_slice(&v.to_be_bytes());
    }
    pub fn u24(&mut self, v: u32) {
        self.buf.extend_from_slice(&v.to_be_bytes()[1..]);
    }
    pub fn u32(&mut self, v: u32) {
        self.buf.extend_from_slice(&v.to_be_bytes());
    }
    pub fn i32(&mut self, v: i32) {
        self.buf.extend_from_slice(&v.to_be_bytes());
    }
    pub fn u64(&mut self, v: u64) {
        self.buf.extend_from_slice(&v.to_be_bytes());
    }
    pub fn bytes(&mut self, b: &[u8]) {
        self.buf.extend_from_slice(b);
    }
    pub fn zeros(&mut self, n: usize) {
        self.buf.resize(self.buf.len() + n, 0);
    }
    pub fn patch_u32(&mut self, at: usize, v: u32) {
        self.buf[at..at + 4].copy_from_slice(&v.to_be_bytes());
    }

    /// Unity transformation matrix used by mvhd/tkhd.
    pub fn matrix(&mut self) {
        for v in [0x0001_0000u32, 0, 0, 0, 0x0001_0000, 0, 0, 0, 0x4000_0000] {
            self.u32(v);
        }
    }
}
