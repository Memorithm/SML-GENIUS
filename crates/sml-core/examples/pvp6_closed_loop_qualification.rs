//! Complete closed-loop destination qualification, not a timing endpoint.
#[path = "../tests/support/a4_closed_loop.rs"]
mod fixture;
fn main() {
    print!("{}", fixture::qualification_report());
}
