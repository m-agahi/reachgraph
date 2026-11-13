//! Task 2068 phase 1: calls in the source against edges in the index.
mod empty;

macro_rules! twice {
    ($e:expr) => {
        $e + $e
    };
}

fn helper_a(x: u32) -> u32 {
    x + 1
}
fn helper_b(x: u32) -> u32 {
    x + 2
}
fn helper_c(x: u32) -> u32 {
    x + 3
}
fn helper_d(x: u32) -> u32 {
    x + 4
}
fn helper_e(x: u32) -> u32 {
    x + 5
}

pub fn shapes(v: Vec<u32>) -> u32 {
    let a = helper_a(1);
    let b: Vec<u32> = v.clone().into_iter().map(|x| helper_b(x)).collect();
    let c: Vec<u32> = v.into_iter().map(helper_c).collect();
    let f: fn(u32) -> u32 = helper_d;
    let d = f(1);
    let e = twice!(helper_e(1));
    a + b.len() as u32 + c.len() as u32 + d + e
}
