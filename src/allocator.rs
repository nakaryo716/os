extern crate alloc;

use core::{
    alloc::{GlobalAlloc, Layout},
    borrow::BorrowMut,
    cell::RefCell,
    cmp::max,
    fmt::{self},
    mem::size_of,
    ops::DerefMut,
    ptr::null_mut,
};

use alloc::boxed::Box;

use crate::{
    results::Result,
    uefi::{EfiMemoryDescriptor, MemoryMapHolder, MemoryType},
};

pub fn round_up_to_nearest_pow2(v: usize) -> Result<usize> {
    1usize
        .checked_shl(usize::BITS - v.wrapping_sub(1).leading_zeros())
        .ok_or("Out of range")
}

pub struct Header {
    next_header: Option<Box<Header>>,
    size: usize,
    is_allocated: bool,
    _reserved: usize,
}

const HEADER_SIZE: usize = size_of::<Header>();

#[allow(clippy::assertions_on_constants)]
const _: () = assert!(HEADER_SIZE == 32);

// HEADER_SIZE must be power of 2
const _: () = assert!(HEADER_SIZE.count_ones() == 1);

pub const LAYOUT_PAGE_4K: Layout = unsafe { Layout::from_size_align_unchecked(4096, 4096) };

impl Header {
    fn can_provide(&self, size: usize, align: usize) -> bool {
        self.size >= size + HEADER_SIZE * 2 + align
    }

    fn is_allocated(&self) -> bool {
        self.is_allocated
    }

    fn end_addr(&self) -> usize {
        self as *const Header as usize + self.size
    }

    unsafe fn new_from_addr(addr: usize) -> Box<Header> {
        let ptr = addr as *mut Header;
        ptr.write(Header {
            next_header: None,
            size: 0,
            is_allocated: false,
            _reserved: 0,
        });
        Box::from_raw(ptr)
    }

    unsafe fn from_allocated_region(addr: *mut u8) -> Box<Header> {
        let ptr = addr.sub(HEADER_SIZE) as *mut Header;
        Box::from_raw(ptr)
    }

    fn provide(&mut self, size: usize, align: usize) -> Option<*mut u8> {
        let size = max(round_up_to_nearest_pow2(size).ok()?, HEADER_SIZE);
        let align = max(align, HEADER_SIZE);

        // verify can provide and not allocated
        if self.is_allocated() || !self.can_provide(size, align) {
            return None;
        }

        let mut size_used = 0;
        // decide header addr
        let allocated_addr = (self.end_addr() - size) & !(align - 1);

        // new Header from addr
        let mut header_for_allocated =
            unsafe { Header::new_from_addr(allocated_addr - HEADER_SIZE) };
        header_for_allocated.is_allocated = true;
        header_for_allocated.size = size + HEADER_SIZE;
        size_used += header_for_allocated.size;
        header_for_allocated.next_header = self.next_header.take();

        // padding header
        if header_for_allocated.end_addr() != self.end_addr() {
            // new Header padding
            let mut header_for_padding =
                unsafe { Header::new_from_addr(header_for_allocated.end_addr()) };

            // init
            header_for_padding.is_allocated = false;
            header_for_padding.size = self.end_addr() - header_for_allocated.end_addr();
            size_used += header_for_padding.size;

            // link header
            header_for_padding.next_header = header_for_allocated.next_header.take();
            header_for_allocated.next_header = Some(header_for_padding);
        }

        // shrink self
        assert!(self.size >= size_used + HEADER_SIZE);
        self.size -= size_used;
        self.next_header = Some(header_for_allocated);
        Some(allocated_addr as *mut u8)
    }
}

impl Drop for Header {
    fn drop(&mut self) {
        panic!("Header should not be dropped");
    }
}

impl fmt::Debug for Header {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Header @{:#018X}{{size:{:#018X}, is_allocated: {}}}",
            self as *const Header as usize, self.size, self.is_allocated,
        )
    }
}

pub struct FirstFitAllocator {
    first_header: RefCell<Option<Box<Header>>>,
}

#[global_allocator]
pub static ALLOCATOR: FirstFitAllocator = FirstFitAllocator {
    first_header: RefCell::new(None),
};

unsafe impl Sync for FirstFitAllocator {}

unsafe impl GlobalAlloc for FirstFitAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        self.alloc_with_options(layout)
    }

    unsafe fn dealloc(&self, ptr: *mut u8, _layout: Layout) {
        let mut header = Header::from_allocated_region(ptr);
        header.is_allocated = false;
        Box::leak(header);
    }
}

impl FirstFitAllocator {
    pub fn alloc_with_options(&self, layout: Layout) -> *mut u8 {
        let mut header = self.first_header.borrow_mut();
        let mut header = header.deref_mut();

        loop {
            match header {
                Some(e) => match e.provide(layout.size(), layout.align()) {
                    Some(ptr) => return ptr,
                    None => {
                        header = e.next_header.borrow_mut();
                        continue;
                    }
                },
                None => return null_mut::<u8>(),
            }
        }
    }

    pub fn init_with_mmap(&self, memory_map: &MemoryMapHolder) {
        for desc in memory_map.iter() {
            if desc.memory_type() != MemoryType::CONVENTIONAL_MEMORY {
                continue;
            }
            self.add_from_descriptor(desc)
        }
    }

    fn add_from_descriptor(&self, desc: &EfiMemoryDescriptor) {
        let mut start_addr = desc.physical_start() as usize;
        let mut size = desc.number_of_pages() as usize * 4096;

        if start_addr == 0 {
            start_addr += 4096;
            size = size.saturating_sub(4096);
        }

        if size <= 4096 {
            return;
        }

        let mut header = unsafe { Header::new_from_addr(start_addr) };
        header.next_header = None;
        header.is_allocated = false;
        header.size = size;

        let mut first_header = self.first_header.borrow_mut();
        let prev_first = first_header.replace(header);
        drop(first_header);

        let mut header = self.first_header.borrow_mut();
        header.as_mut().unwrap().next_header = prev_first;
    }
}
