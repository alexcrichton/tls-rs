use libc::types::os::arch::extra::{DWORD, LPVOID, BOOL};

pub type Key = DWORD;

pub unsafe fn create() -> Key {
    const TLS_OUT_OF_INDEXES: DWORD = 0xFFFFFFFF;
    let key = TlsAlloc();
    assert!(key != TLS_OUT_OF_INDEXES);
    return key;
}

pub unsafe fn set(key: Key, value: *mut u8) {
    assert!(TlsSetValue(key, value as LPVOID) != 0)
}

pub unsafe fn get(key: Key) -> *mut u8 {
    TlsGetValue(key) as *mut u8
}

pub unsafe fn destroy(key: Key) {
    assert!(TlsFree(key) != 0);
}

extern "system" {
    fn TlsAlloc() -> DWORD;
    fn TlsFree(dwTlsIndex: DWORD) -> BOOL;
    fn TlsGetValue(dwTlsIndex: DWORD) -> LPVOID;
    fn TlsSetValue(dwTlsIndex: DWORD, lpTlsvalue: LPVOID) -> BOOL;
}
