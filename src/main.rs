mod board;
mod evaluation;
mod movegen;
mod pst;
mod search;
mod syzygy;
mod time;
mod tt;
mod types;
mod uci;

fn main() {
    board::init();
    uci::run();
}
