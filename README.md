# tls-rs

[![Build Status](https://travis-ci.org/alexcrichton/tls-rs.svg?branch=master)](https://travis-ci.org/alexcrichton/tls-rs)

[Documentation](http://alexcrichton.com/tls-rs/tls/index.html)

An experiment of implementing thread-local-storage in Rust. This crate only
binds to *native* TLS implementations such as LLVM's `thread_local` support or
OS-based tls (aka `TlsAlloc` or `pthread_key_create`)

I'd like to emphasize that this crate is **experimental** at this time. APIs may
change and such. Hopefully one day this will go away altogether once an
implementation is in the stdlib!

## Usage

```toml
[dependencies.tls]
git = "https://github.com/alexcrichton/tls-rs"
```

```rust
#![feature(phase)]
#[phase(plugin, link)]
extern crate tls;

scoped_tls!(static FOO: uint)

fn main() {
    FOO.with(&1, || {
        let foo_val = FOO.with(|x| *x.unwrap());
        println!("{}", foo_val);
    });
}
```
