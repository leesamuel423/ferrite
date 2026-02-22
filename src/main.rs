#![allow(dead_code)]

mod board;
mod evaluation;
mod pst;
mod tt;
mod types;

fn main() {
    board::init();
    println!("Chess engine initialized");
}
