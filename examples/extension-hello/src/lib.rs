#![no_std]

// Minimal global allocator using a static bump arena.
const HEAP_SIZE: usize = 64 * 1024; // 64 KB
static mut HEAP: [u8; HEAP_SIZE] = [0u8; HEAP_SIZE];

struct BumpAlloc;

unsafe impl core::alloc::GlobalAlloc for BumpAlloc {
    unsafe fn alloc(&self, layout: core::alloc::Layout) -> *mut u8 {
        static mut OFFSET: usize = 0;
        unsafe {
            let align = layout.align();
            let size = layout.size();
            let offset = core::ptr::addr_of_mut!(OFFSET);
            let aligned = (*offset + align - 1) & !(align - 1);
            if aligned + size > HEAP_SIZE {
                return core::ptr::null_mut();
            }
            *offset = aligned + size;
            core::ptr::addr_of_mut!(HEAP).cast::<u8>().add(aligned)
        }
    }

    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: core::alloc::Layout) {}
}

#[global_allocator]
static ALLOCATOR: BumpAlloc = BumpAlloc;

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}

// Host imports
unsafe extern "C" {
    fn __uncode_host_register_hook(api_handle: i32, name_ptr: *const u8, name_len: i32);
    fn __uncode_host_log(level: i32, msg_ptr: *const u8, msg_len: i32);
}

extern crate alloc;

use alloc::alloc::Layout;
use core::alloc::GlobalAlloc;

fn alloc_bytes(size: usize) -> *mut u8 {
    unsafe { ALLOCATOR.alloc(Layout::from_size_align(size, 4).unwrap()) }
}

fn write_bytes(data: &[u8]) -> *const u8 {
    let ptr = alloc_bytes(data.len());
    unsafe {
        let slice = core::slice::from_raw_parts_mut(ptr, data.len());
        slice.copy_from_slice(data);
    }
    ptr
}

fn host_log(level: i32, msg: &str) {
    let ptr = write_bytes(msg.as_bytes());
    unsafe { __uncode_host_log(level, ptr, msg.len() as i32) };
}

#[unsafe(no_mangle)]
pub extern "C" fn __uncode_init(api_handle: i32) {
    host_log(2, "hello-world extension initializing");

    // Register for session_start hook
    let hook_name = b"session_start";
    let ptr = write_bytes(hook_name);
    unsafe { __uncode_host_register_hook(api_handle, ptr, hook_name.len() as i32) };

    host_log(2, "hello-world extension initialized");
}

#[unsafe(no_mangle)]
pub extern "C" fn __uncode_on_hook(
    _ctx_ptr: i32,
    _ctx_len: i32,
    _out_ptr: i32,
) -> i32 {
    host_log(2, "hello-world: hook fired");
    0 // Continue
}

#[unsafe(no_mangle)]
pub extern "C" fn __uncode_tool_execute(
    _name_ptr: i32,
    _name_len: i32,
    _args_ptr: i32,
    _args_len: i32,
    _out_ptr: i32,
) -> i32 {
    host_log(2, "hello-world: tool executed");
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn __uncode_allocate(size: i32) -> i32 {
    alloc_bytes(size as usize) as i32
}

#[unsafe(no_mangle)]
pub extern "C" fn __uncode_deallocate(_ptr: i32, _size: i32) {
    // Bump allocator — no-op
}
