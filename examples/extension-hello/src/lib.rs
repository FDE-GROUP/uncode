//! uncode WASM 扩展示例 — hello-world
//!
//! 端到端演示 uncode 扩展 ABI 的完整用法：
//! - `__uncode_init`: 初始化入口，注册钩子 + 自定义工具
//! - `__uncode_on_hook`: 钩子回调，在 session_start 时打印日志
//! - `__uncode_tool_execute`: 工具执行回调，LLM 可调用 `hello_greet` 工具
//! - `__uncode_allocate` / `__uncode_deallocate`: 线性内存分配器
//!
//! 注册的工具 `hello_greet` 接受 `{"name": "..."}` 参数，返回问候语。
//! LLM 在 agent 执行中会看到此工具并可以调用它。
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

// ── 宿主导入函数（由 uncode WASM 运行时提供） ──

unsafe extern "C" {
    /// 注册生命周期钩子。
    fn __uncode_host_register_hook(handle: i32, name_ptr: *const u8, name_len: i32);

    /// 注册自定义工具。json 为 ExtensionToolMetadata 的 JSON 序列化。
    /// 返回 tool_id（>=0）或 -1 表示失败。
    fn __uncode_host_register_tool(handle: i32, json_ptr: *const u8, json_len: i32) -> i32;

    /// 通过宿主日志系统输出日志。level: 0=trace, 1=debug, 2=info, 3=warn, 4=error
    fn __uncode_host_log(level: i32, msg_ptr: *const u8, msg_len: i32);
}

extern crate alloc;

use core::alloc::GlobalAlloc;
use core::alloc::Layout;

// ── 工具元数据（JSON 常量） ──

/// `hello_greet` 工具的注册元数据。
/// LLM 看到 description 后会在需要时主动调用此工具。
const TOOL_META_JSON: &[u8] = br#"{"name":"hello_greet","description":"Generate a greeting from the hello-world WASM extension. Use this when you want to say hello or greet someone.","parameters":{"type":"object","properties":{"name":{"type":"string","description":"The name to greet"}},"required":["name"]},"sequential":false}"#;

// ── 辅助函数 ──

/// 从 WASM 线性内存中分配指定大小的空间。
fn alloc_bytes(size: usize) -> *mut u8 {
    unsafe { ALLOCATOR.alloc(Layout::from_size_align(size, 4).unwrap()) }
}

/// 将字节切片写入 WASM 线性内存，返回指针。
fn write_bytes(data: &[u8]) -> *const u8 {
    let ptr = alloc_bytes(data.len());
    unsafe {
        core::ptr::copy_nonoverlapping(data.as_ptr(), ptr, data.len());
    }
    ptr
}

/// 通过宿主导入输出日志。
fn host_log(level: i32, msg: &str) {
    let ptr = write_bytes(msg.as_bytes());
    unsafe { __uncode_host_log(level, ptr, msg.len() as i32) };
}

/// 从 JSON 字节中提取指定 key 的字符串值。
/// 仅处理 `"key":"value"` 模式，用于 no_std 环境下轻量 JSON 解析。
fn extract_json_string<'a>(json: &'a [u8], key: &[u8]) -> Option<&'a [u8]> {
    let mut i = 0;
    while i + key.len() + 4 <= json.len() {
        // 查找 "key"
        if json[i] == b'"'
            && json[i + 1..].starts_with(key)
            && json.get(i + 1 + key.len()) == Some(&b'"')
        {
            let mut j = i + 2 + key.len(); // 跳过 "key"
            // 跳过冒号和空白
            while j < json.len() && matches!(json[j], b':' | b' ' | b'\t' | b'\n' | b'\r') {
                j += 1;
            }
            if j >= json.len() || json[j] != b'"' {
                i += 1;
                continue;
            }
            j += 1; // 跳过开头引号
            let start = j;
            while j < json.len() && json[j] != b'"' {
                j += 1;
            }
            return Some(&json[start..j]);
        }
        i += 1;
    }
    None
}

// ── WASM 模块导出（宿主调用这些函数） ──

/// 初始化入口。注册钩子和自定义工具。
#[unsafe(no_mangle)]
pub extern "C" fn __uncode_init(api_handle: i32) {
    host_log(2, "hello-world 扩展初始化中");

    // 注册 session_start 钩子
    let hook_name = b"session_start";
    let ptr = write_bytes(hook_name);
    unsafe { __uncode_host_register_hook(api_handle, ptr, hook_name.len() as i32) };

    // 注册 hello_greet 工具，LLM 在 agent 执行中可调用
    let tool_ptr = write_bytes(TOOL_META_JSON);
    let tool_id =
        unsafe { __uncode_host_register_tool(api_handle, tool_ptr, TOOL_META_JSON.len() as i32) };
    if tool_id >= 0 {
        host_log(2, "hello-world: hello_greet 工具注册成功");
    } else {
        host_log(3, "hello-world: hello_greet 工具注册失败");
    }

    host_log(2, "hello-world 扩展初始化完成");
}

/// 钩子回调。session_start 时触发，仅打印日志。
///
/// 返回: 0 = Continue（不拦截，继续正常流程）。
#[unsafe(no_mangle)]
pub extern "C" fn __uncode_on_hook(_ctx_ptr: i32, _ctx_len: i32, _out_ptr: i32) -> i32 {
    host_log(2, "hello-world: session_start 钩子已触发");
    0
}

/// 工具执行回调。LLM 调用 `hello_greet` 时触发。
///
/// 流程:
/// 1. 从 WASM 内存读取工具名和参数 JSON
/// 2. 提取 `name` 字段（未找到则默认 "world"）
/// 3. 构造问候语 JSON 写入 out_ptr
/// 4. 返回结果字节长度
#[unsafe(no_mangle)]
pub extern "C" fn __uncode_tool_execute(
    name_ptr: i32,
    name_len: i32,
    args_ptr: i32,
    args_len: i32,
    out_ptr: i32,
) -> i32 {
    // 读取工具名
    let name = unsafe { core::slice::from_raw_parts(name_ptr as *const u8, name_len as usize) };

    if name != b"hello_greet" {
        host_log(3, "hello-world: 未知工具调用");
        return 0;
    }

    // 读取参数 JSON，提取 name 字段
    let args = unsafe { core::slice::from_raw_parts(args_ptr as *const u8, args_len as usize) };
    let greet_name = extract_json_string(args, b"name").unwrap_or(b"world");

    // 构造结果: {"result":"Hello, {name}! Greetings from uncode WASM extension."}
    let out = out_ptr as *mut u8;
    let prefix = br#"{"result":"Hello, "#;
    let suffix = br#"! Greetings from uncode WASM extension."}"#;

    let mut offset = 0usize;
    unsafe {
        core::ptr::copy_nonoverlapping(prefix.as_ptr(), out, prefix.len());
        offset += prefix.len();
        core::ptr::copy_nonoverlapping(greet_name.as_ptr(), out.add(offset), greet_name.len());
        offset += greet_name.len();
        core::ptr::copy_nonoverlapping(suffix.as_ptr(), out.add(offset), suffix.len());
        offset += suffix.len();
    }

    host_log(1, "hello-world: hello_greet 工具已执行");
    offset as i32
}

/// 在 WASM 线性内存中分配空间，供宿主写入数据。
#[unsafe(no_mangle)]
pub extern "C" fn __uncode_allocate(size: i32) -> i32 {
    alloc_bytes(size as usize) as i32
}

/// 释放 WASM 线性内存（bump allocator 下为空操作）。
#[unsafe(no_mangle)]
pub extern "C" fn __uncode_deallocate(_ptr: i32, _size: i32) {}
