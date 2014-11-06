#![feature(phase)]

#[phase(plugin, link)] extern crate tls;

scoped_tls!(static BAR: int)

fn main() {
}
