//! uncode WASM 扩展示例 — hello-world
//!
//! 演示如何实现 uncode 扩展 ABI：
//! - `__uncode_init`: 初始化入口，注册钩子/工具/命令
//! - `__uncode_on_hook`: 钩子回调，接收生命周期事件
//! - `__uncode_allocate` / `__uncode_deallocate`: 线性内存分配器
//!
//! 构建: `cargo build --release --target wasm32-unknown-unknown`
//! 部署: `cp target/wasm32-unknown-unknown/release/uncode_ext_hello.wasm ~/.uncode/extensions/hello.wasm`

#![no_std]

// ── 全局内存分配器（bump allocator，仅向上递增指针） ──

const HEAP_SIZE: usize = 64 * 1024; // 64 KB 静态堆
static mut HEAP: [u8; HEAP_SIZE] = [0u8; HEAP_SIZE];

struct BumpAlloc;

unsafe impl core::alloc::GlobalAlloc for BumpAlloc {
    unsafe fn alloc(&self, layout: core::alloc::Layout) -> *mut u8 {
        static mut OFFSET: usize = 0;
        unsafe {
            let align = layout.align();
            let size = layout.size();
            let offset = core::ptr::addr_of_mut!(OFFSET);
            // 按对齐要求向上取整
            let aligned = (*offset + align - 1) & !(align - 1);
            if aligned + size > HEAP_SIZE {
                return core::ptr::null_mut(); // 堆空间不足
            }
            *offset = aligned + size;
            core::ptr::addr_of_mut!(HEAP).cast::<u8>().add(aligned)
        }
    }

    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: core::alloc::Layout) {
        // bump allocator 不支持释放
    }
}

#[global_allocator]
static ALLOCATOR: BumpAlloc = BumpAlloc;

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}

// ── 宿主导入函数（由 uncode WASM 运行时提供） ──

unsafe extern "C" {
    /// 注册生命周期钩子。level: 0=trace, 1=debug, 2=info, 3=warn, 4=error
    fn __uncode_host_register_hook(api_handle: i32, name_ptr: *const u8, name_len: i32);
    /// 通过宿主日志系统输出日志。
    fn __uncode_host_log(level: i32, msg_ptr: *const u8, msg_len: i32);
}

extern crate alloc;

use alloc::alloc::Layout;
use core::alloc::GlobalAlloc;

/// 从 WASM 线性内存中分配指定大小的空间。
fn alloc_bytes(size: usize) -> *mut u8 {
    unsafe { ALLOCATOR.alloc(Layout::from_size_align(size, 4).unwrap()) }
}

/// 将字节切片写入 WASM 线性内存，返回指针。
fn write_bytes(data: &[u8]) -> *const u8 {
    let ptr = alloc_bytes(data.len());
    unsafe {
        let slice = core::slice::from_raw_parts_mut(ptr, data.len());
        slice.copy_from_slice(data);
    }
    ptr
}

/// 通过宿主导入输出日志。
fn host_log(level: i32, msg: &str) {
    let ptr = write_bytes(msg.as_bytes());
    unsafe { __uncode_host_log(level, ptr, msg.len() as i32) };
}

// ── WASM 模块导出（宿主调用这些函数） ──

/// 初始化入口。扩展在此注册钩子、工具、命令。
#[unsafe(no_mangle)]
pub extern "C" fn __uncode_init(api_handle: i32) {
    host_log(2, "hello-world 扩展初始化中");

    // 注册 session_start 钩子
    let hook_name = b"session_start";
    let ptr = write_bytes(hook_name);
    unsafe { __uncode_host_register_hook(api_handle, ptr, hook_name.len() as i32) };

    host_log(2, "hello-world 扩展初始化完成");
}

/// 钩子回调。当注册的生命周期事件触发时，宿主调用此函数。
///
/// 参数:
/// - `ctx_ptr` / `ctx_len`: 序列化为 JSON 的 HookContext（含 session_id 等）
/// - `out_ptr`: 输出缓冲区指针，用于写入 JSON 格式的 HookResult
///
/// 返回: 输出缓冲区中结果 JSON 的字节长度，0 表示 Continue。
#[unsafe(no_mangle)]
pub extern "C" fn __uncode_on_hook(
    _ctx_ptr: i32,
    _ctx_len: i32,
    _out_ptr: i32,
) -> i32 {
    host_log(2, "hello-world: 钩子已触发");
    0 // Continue — 不拦截，继续正常流程
}

/// 工具执行回调。当扩展注册的工具被 LLM 调用时触发。
///
/// 参数:
/// - `name_ptr` / `name_len`: 工具名称
/// - `args_ptr` / `args_len`: JSON 格式的工具参数
/// - `out_ptr`: 输出缓冲区指针
///
/// 返回: 输出缓冲区中结果 JSON 的字节长度。
#[unsafe(no_mangle)]
pub extern "C" fn __uncode_tool_execute(
    _name_ptr: i32,
    _name_len: i32,
    _args_ptr: i32,
    _args_len: i32,
    _out_ptr: i32,
) -> i32 {
    host_log(2, "hello-world: 工具已执行");
    0
}

/// 在 WASM 线性内存中分配空间，供宿主写入数据。
#[unsafe(no_mangle)]
pub extern "C" fn __uncode_allocate(size: i32) -> i32 {
    alloc_bytes(size as usize) as i32
}

/// 释放 WASM 线性内存（bump allocator 下为空操作）。
#[unsafe(no_mangle)]
pub extern "C" fn __uncode_deallocate(_ptr: i32, _size: i32) {}
